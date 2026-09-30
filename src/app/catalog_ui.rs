//! Catalog browser: every loaded pack with its problems, each hinge's facts,
//! a test bench that runs the real installation checks on a sample door, and
//! "Add to project", which pins the chosen variant into the project.
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::hinge_ui::{field_label, pair_segmented, short_mm, warn_text, warning_callout};
use crate::icons::Icon;
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::catalog_pack::{
    CatalogRegistry, HingeFamily, HingeVariant, LoadedPack, Pack, PackIssue, ReviewStatus,
    Severity, snapshot,
};
use plan_my_cabinet::domain::HingeArm;
use plan_my_cabinet::hardware_catalog::{self, Trust};
use plan_my_cabinet::hinge_installation::{self, BenchSetup};

pub(crate) struct CatalogDialog {
    pack: usize,
    family: usize,
    variant: usize,
    door: String,
    side: String,
    inset_depth: String,
    pair: Option<(Length, Length)>,
    message: Option<(String, bool)>,
    import: Option<mpsc::Receiver<Option<PathBuf>>>,
    chrome: ModalChrome,
}

enum Intent {
    Reload,
    OpenFolder,
    Import,
}

impl CatalogDialog {
    pub(crate) fn new() -> Self {
        Self {
            pack: 0,
            family: 0,
            variant: 0,
            door: "18".into(),
            side: "18".into(),
            inset_depth: "18".into(),
            pair: None,
            message: None,
            import: None,
            chrome: ModalChrome::new(egui::Id::new("catalog-dialog"))
                .icon(Icon::Hinge)
                .first_focus(egui::Id::new(("catalog-bench", "catalogs-bench-door")))
                .width(880.0),
        }
    }

    /// Select a pack, hinge and variant by id and product code.
    pub(crate) fn show_variant(
        &mut self,
        registry: &CatalogRegistry,
        (pack_id, family_id, code): (&str, &str, &str),
    ) {
        let found = registry.packs.iter().enumerate().find_map(|(p, loaded)| {
            let pack = loaded.usable().filter(|pack| pack.id == pack_id)?;
            pack.hinges.iter().enumerate().find_map(|(f, family)| {
                (family.id == family_id).then_some(())?;
                let v = family.variants.iter().position(|v| v.code == code)?;
                Some((p, f, v))
            })
        });
        if let Some((pack, family, variant)) = found {
            self.pack = pack;
            self.select_family(family);
            self.variant = variant;
        }
    }

    fn select_family(&mut self, family: usize) {
        self.family = family;
        self.variant = 0;
        self.pair = None;
    }
}

pub(crate) fn language_tag(localizer: &Localizer) -> &'static str {
    match localizer.language() {
        Language::En => "en",
        Language::PtBr => "pt-BR",
    }
}

pub(crate) fn arm_label(localizer: &Localizer, arm: HingeArm) -> String {
    localizer.text(match arm {
        HingeArm::FullOverlay => "arm-full-overlay",
        HingeArm::HalfOverlay => "arm-half-overlay",
        HingeArm::Inset => "arm-inset",
    })
}

/// The letter the K table gives for an arm: R (overlay) or F (inset gap).
pub(crate) fn table_letter(arm: HingeArm) -> &'static str {
    if arm.is_inset() { "F" } else { "R" }
}

fn issue_text(localizer: &Localizer, issue: &PackIssue) -> String {
    let message = localizer.text(&format!("catalogs-issue-{}", issue.kind.code()));
    let mut text = if issue.path.is_empty() {
        message
    } else {
        format!("{} — {message}", issue.path)
    };
    if let Some(detail) = issue.kind.detail() {
        text.push_str(&format!(" ({detail})"));
    }
    text
}

fn status_chip(ui: &mut egui::Ui, localizer: &Localizer, loaded: &LoadedPack) {
    let (key, fill, ink) = if loaded.errors() > 0 {
        (
            "catalogs-status-errors",
            egui::Color32::from_rgb(250, 228, 224),
            tw::DANGER,
        )
    } else if loaded
        .pack
        .as_ref()
        .is_some_and(|p| p.review.status == ReviewStatus::Draft)
    {
        ("catalogs-status-draft", tw::WARN_BG, tw::WARN_INK)
    } else if loaded.is_bundled() {
        ("catalogs-status-reviewed", tw::OK_BG, tw::OK)
    } else {
        ("catalogs-status-user", tw::ACCENT_BG, tw::ACCENT_DARK)
    };
    tw::chip(ui, &localizer.text(key), fill, ink);
}

fn user_folder(registry: &CatalogRegistry) -> Option<PathBuf> {
    registry.user_dir.clone()
}

/// Open a folder in the platform file manager, creating it first.
fn reveal_folder(folder: &Path) -> bool {
    if std::fs::create_dir_all(folder).is_err() {
        return false;
    }
    #[cfg(target_os = "macos")]
    let command = "open";
    #[cfg(target_os = "linux")]
    let command = "xdg-open";
    #[cfg(target_os = "windows")]
    let command = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let command = "";
    std::process::Command::new(command)
        .arg(folder)
        .status()
        .is_ok_and(|status| status.success())
}

/// Copy a chosen pack into the user folder, keeping its file name.
fn import_pack(source: &Path, folder: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let name = source
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;
    let target = folder.join(name);
    if target != source {
        std::fs::copy(source, &target)?;
    }
    Ok(target)
}

fn fact_rows(
    localizer: &Localizer,
    pack: &Pack,
    family: &HingeFamily,
    variant: &HingeVariant,
) -> Vec<(String, String)> {
    let mm = |v: Length| short_mm(localizer, v);
    let unknown = localizer.text("catalogs-unknown");
    let mut rows = vec![
        (
            localizer.text("catalogs-facts-code"),
            format!(
                "{} / {}",
                variant.code,
                variant.plate_code.as_deref().unwrap_or("—")
            ),
        ),
        (
            localizer.text("catalogs-facts-cup"),
            format!("Ø{} × {} mm", mm(family.cup_diameter), mm(family.cup_depth)),
        ),
        (
            localizer.text("catalogs-facts-door"),
            format!(
                "{}–{} mm",
                mm(family.door_thickness_min),
                mm(family.door_thickness_max)
            ),
        ),
        (
            localizer.text("catalogs-facts-plate"),
            format!(
                "H={} · {} / {} mm",
                mm(variant.plate_height),
                family.plate_hole_pitch.map_or(unknown.clone(), mm),
                mm(family.plate_front_offset)
            ),
        ),
        (
            localizer.text("catalogs-facts-opening"),
            family
                .opening_degrees
                .map_or(unknown.clone(), |d| format!("{d}°")),
        ),
        (
            localizer.text("catalogs-facts-fasteners"),
            family.fasteners.clone().unwrap_or(unknown),
        ),
        (
            localizer.text("catalogs-facts-source"),
            format!("{} · {}", family.source.title, family.source.revision),
        ),
    ];
    if let Some(notes) = &family.notes {
        rows.push((localizer.text("catalogs-facts-notes"), notes.clone()));
    }
    rows.push((
        localizer.text("catalogs-facts-pack"),
        format!("{} {}", pack.manufacturer, pack.version),
    ));
    rows
}

fn rows_ui(ui: &mut egui::Ui, salt: &str, rows: &[(String, String)]) {
    egui::Grid::new(ui.id().with(salt))
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(egui::RichText::new(label).size(12.0).color(tw::MUTED));
                ui.add(egui::Label::new(tw::mono(value, 11.5).color(tw::TEXT)).wrap());
                ui.end_row();
            }
        });
}

fn length_field(text: &str) -> Option<Length> {
    let value = parse_length(text, Unit::Mm).ok()?.conversion.exact()?;
    (value.micrometres() > 0).then_some(value)
}

impl DesktopApp {
    pub(crate) fn open_catalog_dialog(&mut self) {
        self.modals.set_catalog(Some(CatalogDialog::new()));
    }

    fn reload_catalogs(&mut self) {
        let dir = self.hardware.catalogs.user_dir.clone();
        self.hardware.catalogs = CatalogRegistry::load(dir.as_deref());
    }

    pub(crate) fn show_catalog_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.modals.take_catalog() else {
            return;
        };
        if let Some(receiver) = &dialog.import
            && let Ok(choice) = receiver.try_recv()
        {
            dialog.import = None;
            if let Some(source) = choice {
                dialog.message = Some(
                    match user_folder(&self.hardware.catalogs)
                        .ok_or_else(|| std::io::Error::other("no user folder"))
                        .and_then(|folder| import_pack(&source, &folder))
                    {
                        Ok(target) => {
                            self.reload_catalogs();
                            if let Some(index) = self.hardware.catalogs.packs.iter().position(|p| {
                            matches!(&p.origin, plan_my_cabinet::catalog_pack::PackOrigin::User(path) if *path == target)
                        }) {
                            dialog.pack = index;
                            dialog.select_family(0);
                        }
                            (self.localizer.text("catalogs-imported"), false)
                        }
                        Err(error) => {
                            let mut args = FluentArgs::new();
                            args.set("error", error.to_string());
                            (
                                self.localizer.format("catalogs-import-failed", Some(&args)),
                                true,
                            )
                        }
                    },
                );
            }
        }
        let registry = &self.hardware.catalogs;
        dialog.pack = dialog.pack.min(registry.packs.len().saturating_sub(1));
        let localizer = &self.localizer;
        let language = language_tag(localizer);
        let CatalogDialog {
            pack: pack_index,
            family: family_index,
            variant: variant_index,
            door,
            side,
            inset_depth,
            pair,
            message,
            import,
            chrome,
        } = &mut dialog;
        let result = chrome.show(
            ctx,
            &localizer.text("catalogs-title"),
            ModalActions {
                cancel: &localizer.text("catalogs-close"),
                confirm: &localizer.text("catalogs-add"),
            },
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let mut intent = None;
                let mut chosen = None;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    // Pack list and folder actions.
                    ui.allocate_ui_with_layout(
                        egui::vec2(230.0, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(230.0);
                            field_label(ui, &localizer.text("catalogs-packs"));
                            for (index, loaded) in registry.packs.iter().enumerate() {
                                let selected = index == *pack_index;
                                let (response, ()) = tw::list_row(
                                    ui,
                                    egui::Id::new(("catalog-pack", index)),
                                    44.0,
                                    if selected {
                                        tw::RowState::Active
                                    } else {
                                        tw::RowState::Normal
                                    },
                                    true,
                                    &loaded.file_name(),
                                    |ui| {
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing.y = 2.0;
                                            let title = loaded.pack.as_ref().map_or_else(
                                                || loaded.file_name(),
                                                |p| format!("{} · {}", p.manufacturer, p.version),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    tw::medium(ui, title, 12.5).color(tw::TEXT),
                                                )
                                                .truncate()
                                                .selectable(false),
                                            );
                                            ui.horizontal(|ui| {
                                                status_chip(ui, localizer, loaded);
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(
                                                            if loaded.is_bundled() {
                                                                localizer.text("catalogs-bundled")
                                                            } else {
                                                                loaded.file_name()
                                                            },
                                                        )
                                                        .size(11.0)
                                                        .color(tw::FAINT),
                                                    )
                                                    .truncate()
                                                    .selectable(false),
                                                );
                                            });
                                        });
                                    },
                                );
                                if response.clicked() && !selected {
                                    *pack_index = index;
                                    *family_index = 0;
                                    *variant_index = 0;
                                    *pair = None;
                                }
                            }
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                if tw::secondary_button(ui, &localizer.text("catalogs-reload"))
                                    .clicked()
                                {
                                    intent = Some(Intent::Reload);
                                }
                                if tw::secondary_button_enabled(
                                    ui,
                                    &localizer.text("catalogs-import"),
                                    import.is_none() && registry.user_dir.is_some(),
                                )
                                .clicked()
                                {
                                    intent = Some(Intent::Import);
                                }
                                if tw::secondary_button_enabled(
                                    ui,
                                    &localizer.text("catalogs-open-folder"),
                                    registry.user_dir.is_some(),
                                )
                                .clicked()
                                {
                                    intent = Some(Intent::OpenFolder);
                                }
                            });
                            if let Some(folder) = &registry.user_dir {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "{} {}",
                                            localizer.text("catalogs-folder-hint"),
                                            folder.display()
                                        ))
                                        .size(11.0)
                                        .color(tw::FAINT),
                                    )
                                    .wrap(),
                                );
                            }
                            if let Some((text, error)) = message.as_ref() {
                                ui.label(egui::RichText::new(text).size(11.5).color(if *error {
                                    tw::DANGER
                                } else {
                                    tw::OK
                                }));
                            }
                        },
                    );
                    // Selected pack: problems, hinge facts and the bench.
                    ui.vertical(|ui| {
                        let Some(loaded) = registry.packs.get(*pack_index) else {
                            return;
                        };
                        if !loaded.issues.is_empty() {
                            let errors = loaded.errors() > 0;
                            warning_callout(ui, false, |ui| {
                                ui.label(
                                    tw::medium(
                                        ui,
                                        localizer.text(if errors {
                                            "catalogs-not-usable"
                                        } else {
                                            "catalogs-warnings"
                                        }),
                                        12.5,
                                    )
                                    .color(tw::WARN_INK),
                                );
                                let mut issues: Vec<_> = loaded.issues.iter().collect();
                                issues.sort_by_key(|i| std::cmp::Reverse(i.kind.severity()));
                                for issue in issues.iter().take(12) {
                                    let text = issue_text(localizer, issue);
                                    if issue.kind.severity() == Severity::Error {
                                        ui.label(
                                            egui::RichText::new(text).size(12.0).color(tw::DANGER),
                                        );
                                    } else {
                                        warn_text(ui, text);
                                    }
                                }
                                if issues.len() > 12 {
                                    ui.label(
                                        egui::RichText::new(format!("+{}", issues.len() - 12))
                                            .size(11.5)
                                            .color(tw::MUTED),
                                    );
                                }
                            });
                        }
                        let Some(pack) = loaded.usable() else {
                            return;
                        };
                        if !pack.slides.is_empty() || !pack.feet.is_empty() {
                            hardware_models(ui, localizer, pack, language);
                        }
                        if pack.hinges.is_empty() {
                            return;
                        }
                        *family_index = (*family_index).min(pack.hinges.len() - 1);
                        let family = &pack.hinges[*family_index];
                        field_label(ui, &localizer.text("catalogs-family"));
                        egui::ComboBox::from_id_salt("catalog-family")
                            .width(ui.available_width())
                            .selected_text(family.name(language))
                            .show_ui(ui, |ui| {
                                for (index, f) in pack.hinges.iter().enumerate() {
                                    if ui
                                        .selectable_label(index == *family_index, f.name(language))
                                        .clicked()
                                    {
                                        *family_index = index;
                                        *variant_index = 0;
                                        *pair = None;
                                    }
                                }
                            });
                        let family = &pack.hinges[*family_index];
                        *variant_index =
                            (*variant_index).min(family.variants.len().saturating_sub(1));
                        if family.variants.is_empty() {
                            return;
                        }
                        field_label(ui, &localizer.text("catalogs-arm"));
                        let mut selected = *variant_index;
                        let labels: Vec<(usize, String)> = family
                            .variants
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (i, arm_label(localizer, v.arm)))
                            .collect();
                        let options: Vec<(usize, &str)> =
                            labels.iter().map(|(i, l)| (*i, l.as_str())).collect();
                        tw::segmented(ui, &mut selected, &options);
                        if selected != *variant_index {
                            *variant_index = selected;
                            *pair = None;
                        }
                        let variant = &family.variants[*variant_index];
                        rows_ui(
                            ui,
                            "catalog-facts",
                            &fact_rows(localizer, pack, family, variant),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.hyperlink_to(
                                egui::RichText::new(localizer.text("hinge-source-review"))
                                    .size(11.5)
                                    .color(tw::ACCENT_DARK),
                                &family.source.url,
                            );
                            if !loaded.is_bundled() {
                                ui.label(
                                    egui::RichText::new(localizer.text("catalogs-user-note"))
                                        .size(11.5)
                                        .color(tw::WARN_INK),
                                );
                            }
                        });
                        // Test bench: the real installation checks on a sample door.
                        ui.add_space(4.0);
                        tw::section_header(ui, &localizer.text("catalogs-bench"));
                        let letter = table_letter(variant.arm);
                        let pairs = variant.k_table.clone();
                        let current = pair.unwrap_or_else(|| {
                            pairs
                                .first()
                                .copied()
                                .unwrap_or((Length::ZERO, Length::ZERO))
                        });
                        field_label(
                            ui,
                            &format!("K / {letter} ({})", localizer.text("catalogs-bench-pairs")),
                        );
                        if let Some(chosen_pair) =
                            pair_segmented(ui, localizer, &pairs, current, true)
                        {
                            *pair = Some(chosen_pair);
                        }
                        let current = pair.unwrap_or(current);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let fields: Vec<(&str, &mut String)> = if variant.arm.is_inset() {
                                vec![
                                    ("catalogs-bench-door", door),
                                    ("catalogs-bench-side", side),
                                    ("catalogs-bench-e", inset_depth),
                                ]
                            } else {
                                vec![("catalogs-bench-door", door), ("catalogs-bench-side", side)]
                            };
                            let width = ((ui.available_width() - 16.0) / 3.0).floor();
                            for (key, text) in fields {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(width, 56.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        let label = localizer.text(key);
                                        field_label(ui, &label);
                                        let invalid = length_field(text).is_none();
                                        tw::unit_field(
                                            ui,
                                            egui::Id::new(("catalog-bench", key)),
                                            &label,
                                            text,
                                            "mm",
                                            invalid
                                                .then(|| localizer.text("hinge-distance-invalid"))
                                                .as_deref(),
                                        );
                                    },
                                );
                            }
                        });
                        let entry = snapshot(pack, family, variant, language);
                        if let (Some(door_thickness), Some(side_thickness), Some(e)) = (
                            length_field(door),
                            length_field(side),
                            if variant.arm.is_inset() {
                                length_field(inset_depth)
                            } else {
                                Some(Length::ZERO)
                            },
                        ) {
                            let status = hinge_installation::bench(
                                &entry,
                                BenchSetup {
                                    door_thickness,
                                    side_thickness,
                                    cup_edge_setback: current.0,
                                    table_value: current.1,
                                    inset_depth: e,
                                },
                            );
                            if status.issues.is_empty()
                                && let Some(r) = &status.references
                            {
                                ui.horizontal(|ui| {
                                    ui.add(crate::icons::icon(Icon::Check, tw::OK, 13.0));
                                    ui.label(
                                        egui::RichText::new(localizer.text("catalogs-bench-ok"))
                                            .size(12.0)
                                            .color(tw::OK),
                                    );
                                });
                                let mm = |v: i128| {
                                    short_mm(
                                        localizer,
                                        Length::from_micrometres(
                                            v.clamp(0, i64::MAX as i128) as i64
                                        ),
                                    )
                                };
                                let mut rows = vec![
                                    (
                                        letter.to_string(),
                                        format!("{} mm", short_mm(localizer, current.1)),
                                    ),
                                    (
                                        localizer.text("catalogs-bench-cup"),
                                        format!("{} mm", mm(r.cup_center_um[0])),
                                    ),
                                    (
                                        localizer.text("catalogs-bench-plate"),
                                        format!("{} mm", mm(r.plate_hole_centers_um[0][0])),
                                    ),
                                ];
                                if r.plate_hole_pitch > Length::ZERO {
                                    rows.push((
                                        localizer.text("hardware-plate-holes"),
                                        format!(
                                            "± {} mm",
                                            short_mm(
                                                localizer,
                                                Length::from_micrometres(
                                                    r.plate_hole_pitch.micrometres() / 2
                                                )
                                            )
                                        ),
                                    ));
                                }
                                rows_ui(ui, "catalog-bench-results", &rows);
                            } else {
                                warning_callout(ui, false, |ui| {
                                    for issue in &status.issues {
                                        warn_text(
                                            ui,
                                            localizer.text(crate::hinge_ui::issue_key(issue)),
                                        );
                                    }
                                });
                            }
                        }
                        chosen = Some(entry);
                    });
                });
                if let Some(intent) = intent {
                    return (Err(intent), false);
                }
                let valid = chosen.is_some();
                (Ok(chosen), valid)
            },
        );
        let chosen = match result.body {
            Err(Intent::Reload) => {
                self.reload_catalogs();
                dialog.message = Some((self.localizer.text("catalogs-reloaded"), false));
                None
            }
            Err(Intent::OpenFolder) => {
                if let Some(folder) = user_folder(&self.hardware.catalogs)
                    && !reveal_folder(&folder)
                {
                    dialog.message = Some((self.localizer.text("catalogs-open-failed"), true));
                }
                None
            }
            Err(Intent::Import) => {
                let (tx, rx) = mpsc::channel();
                dialog.import = Some(rx);
                let picker = rfd::AsyncFileDialog::new()
                    .add_filter("TOML", &["toml"])
                    .pick_file();
                std::thread::spawn(move || {
                    let path = pollster::block_on(picker).map(|h| h.path().to_path_buf());
                    let _ = tx.send(path);
                });
                None
            }
            Ok(chosen) => chosen,
        };
        if actions::decision(A::CancelDialog, result.action == ModalAction::Cancel) {
            dialog.chrome.close(ctx);
            return;
        }
        if actions::decision(A::ConfirmDialog, result.action == ModalAction::Confirm)
            && let Some(entry) = chosen
        {
            let name = entry.name.clone();
            match hardware_catalog::add(&mut self.editor, entry) {
                Ok(_) => {
                    let mut args = FluentArgs::new();
                    args.set("name", name);
                    self.toasts
                        .info(self.localizer.format("catalogs-added", Some(&args)));
                    dialog.chrome.close(ctx);
                    return;
                }
                Err(error) => self.report_edit::<(), _>(Err(error)),
            }
        }
        self.modals.set_catalog(Some(dialog));
    }
}

fn mm_label(value: Length) -> String {
    let um = value.micrometres();
    if um % 1000 == 0 {
        (um / 1000).to_string()
    } else {
        format!("{}", um as f64 / 1000.0)
    }
}

/// Read-only list of a pack's drawer slides and feet. They are added from
/// the Hardware "+" menu (Drawer slides…, Foot…).
fn hardware_models(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    pack: &plan_my_cabinet::catalog_pack::Pack,
    language: &str,
) {
    egui::ScrollArea::vertical()
        .id_salt("catalog-hardware-models")
        .max_height(340.0)
        .show(ui, |ui| {
            if !pack.slides.is_empty() {
                tw::section_header(ui, &localizer.text("slide-list"));
                for family in &pack.slides {
                    ui.label(tw::medium(ui, family.name(language), 12.5).color(tw::TEXT));
                    let lengths = family
                        .variants
                        .iter()
                        .map(|v| mm_label(v.length))
                        .collect::<Vec<_>>()
                        .join(", ");
                    ui.label(
                        egui::RichText::new(format!(
                            "H {} · {} {} (+{} / -{}) · {} {} mm",
                            mm_label(family.height),
                            localizer.text("slide-clearance"),
                            mm_label(family.clearance),
                            mm_label(family.clearance_plus),
                            mm_label(family.clearance_minus),
                            localizer.text("slide-length"),
                            lengths
                        ))
                        .size(11.5)
                        .color(tw::MUTED),
                    );
                }
            }
            if !pack.feet.is_empty() {
                ui.add_space(4.0);
                tw::section_header(ui, &localizer.text("pdf-feet"));
                for family in &pack.feet {
                    ui.label(tw::medium(ui, family.name(language), 12.5).color(tw::TEXT));
                    for variant in &family.variants {
                        let size = family.spec(variant).local_size();
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {} × {} × {} mm",
                                variant.code,
                                mm_label(size[0]),
                                mm_label(size[1]),
                                mm_label(size[2])
                            ))
                            .size(11.5)
                            .color(tw::MUTED),
                        );
                    }
                }
            }
            ui.label(
                egui::RichText::new(localizer.text("catalogs-hardware-models-hint"))
                    .size(11.0)
                    .color(tw::FAINT),
            );
        });
}

/// Trust chip for a pinned catalog record.
pub(crate) fn trust_chip(ui: &mut egui::Ui, localizer: &Localizer, trust: Option<Trust>) {
    let (key, fill, ink) = match trust {
        Some(Trust::Reviewed) => ("hardware-trust-reviewed", tw::OK_BG, tw::OK),
        Some(Trust::UserSupplied) => ("hardware-trust-user", tw::ACCENT_BG, tw::ACCENT_DARK),
        Some(Trust::Generic) => ("hardware-trust-generic", tw::ACCENT_BG, tw::ACCENT_DARK),
        None => ("hardware-trust-none", tw::WARN_BG, tw::WARN_INK),
    };
    tw::chip(ui, &localizer.text(key), fill, ink);
}

#[cfg(test)]
mod tests;
