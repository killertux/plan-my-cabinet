//! Read-only export history for the Handoff workspace. All comparisons come from
//! the receipt read model; this view does not inspect exported files or dispatch actions.

use std::collections::BTreeSet;

use eframe::egui::{self, Color32, RichText};
use fluent_bundle::FluentArgs;
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::export::{ComparisonChange, ComparisonKind, ExportMode};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::icons::{Icon, icon};
use plan_my_cabinet::receipt_read_models::{
    ReceiptCard, ReceiptFreshness, SinceThen, receipt_cards,
};
use plan_my_cabinet::theme_widgets as tw;
use plan_my_cabinet::units::Unit;

/// Ink for "Since then" text on a stale receipt (`#5C3E10`).
const STALE_INK: Color32 = Color32::from_rgb(92, 62, 16);
/// Ink for the "out of date" chip (`#8A520A`).
const OUTDATED_CHIP_INK: Color32 = Color32::from_rgb(138, 82, 10);

/// The Handoff right pane: scrolling history and guidance above a pinned
/// save reminder. `height` is the pane height the host can give us.
pub fn show_panel(ui: &mut egui::Ui, project: &Project, l: &Localizer, width: f32, height: f32) {
    let height = if height.is_finite() {
        height.max(1.0)
    } else {
        600.0
    };
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width.max(1.0), height), egui::Sense::hover());
    let mut pane = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::bottom_up(egui::Align::Min)),
    );
    egui::Frame::new().inner_margin(14).show(&mut pane, |ui| {
        egui::Frame::new()
            .fill(tw::APP)
            .corner_radius(9)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.add(icon(Icon::Folder, tw::MUTED, 16.0));
                    ui.add(
                        egui::Label::new(
                            RichText::new(l.text("handoff-save-note"))
                                .size(12.0)
                                .color(tw::SECONDARY),
                        )
                        .wrap(),
                    );
                });
            });
    });
    pane.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("handoff-receipt-panel")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                show_receipts(ui, project, l);
            });
    });
}

/// Draws the entire history in recorded newest-first order, followed by the
/// shop-review guidance. The host should call this inside Handoff using its
/// current project and UI-language localizer.
pub fn show_receipts(ui: &mut egui::Ui, project: &Project, l: &Localizer) {
    let cards = receipt_cards(project);
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 4,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.vertical(|ui| tw::inspector_heading(ui, &l.text("export-history"), |_| ()));
            })
            .response
            .on_hover_text(l.text("receipt-history-note"));
            if cards.is_empty() {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(l.text("handoff-history-empty"))
                            .size(12.0)
                            .color(tw::FAINT),
                    );
                });
            }
            for card in &cards {
                receipt_card(ui, card, l);
                ui.add_space(8.0);
            }
        });
    ui.add_space(8.0);
    let (line, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        line.x_range(),
        line.center().y,
        egui::Stroke::new(1.0, tw::BORDER_SOFT),
    );
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            tw::inspector_heading(ui, &l.text("handoff-before-send"), |_| ());
            for key in [
                "handoff-send-kerf",
                "handoff-send-order",
                "handoff-send-ids",
            ] {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(4.0, 16.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(dot.center() + egui::vec2(0.0, 1.0), 2.0, tw::FAINT);
                    ui.add(
                        egui::Label::new(
                            RichText::new(l.text(key)).size(12.0).color(tw::SECONDARY),
                        )
                        .wrap(),
                    );
                });
                ui.add_space(2.0);
            }
        });
}

fn receipt_card(ui: &mut egui::Ui, card: &ReceiptCard, l: &Localizer) {
    let muted = card.superseded;
    let outdated = card.packet == ReceiptFreshness::Outdated;
    let frame = if muted {
        egui::Frame::new()
            .fill(tw::APP)
            .corner_radius(9)
            .inner_margin(12)
    } else {
        egui::Frame::new()
            .fill(tw::CARD)
            .stroke(egui::Stroke::new(
                1.0,
                if outdated {
                    tw::WARN_STROKE
                } else {
                    tw::BORDER_SOFT
                },
            ))
            .corner_radius(9)
            .inner_margin(12)
    };
    let shown = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if muted {
                    ui.label(
                        RichText::new(l.text("handoff-superseded"))
                            .size(10.5)
                            .color(tw::FAINT),
                    );
                } else {
                    let (key, fill, ink) = match card.packet {
                        ReceiptFreshness::Current => {
                            ("handoff-chip-current", tw::OK_BG, tw::OK_INK)
                        }
                        ReceiptFreshness::Outdated => {
                            ("handoff-chip-outdated", tw::WARN_BG, OUTDATED_CHIP_INK)
                        }
                        ReceiptFreshness::Unavailable | ReceiptFreshness::NotIncluded => {
                            ("handoff-chip-unknown", tw::VIEWPORT, tw::SECONDARY)
                        }
                    };
                    tw::chip(ui, &l.text(key), fill, ink);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let name = if muted {
                        tw::medium(ui, &card.filename, 13.0).color(tw::TEXT_2)
                    } else {
                        tw::semibold(ui, &card.filename, 13.0).color(tw::TEXT)
                    };
                    ui.add(egui::Label::new(name).truncate())
                        .on_hover_text(card.path.display().to_string());
                });
            });
        });
        ui.label(
            RichText::new(meta_line(card, l))
                .size(11.5)
                .color(tw::MUTED),
        );
        if !muted {
            let (text, color) = since_then_summary(card, l);
            ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).wrap());
            ui.label(
                tw::mono(format!("sha256 {}", short_hash(&card.file_sha256)), 10.0)
                    .color(tw::FAINT),
            );
        }
    });
    shown.response.on_hover_ui(|ui| {
        ui.set_max_width(420.0);
        for line in card_lines(card, l) {
            ui.label(RichText::new(line).size(11.5));
        }
        ui.separator();
        ui.label(
            RichText::new(l.text("receipt-since-then"))
                .size(11.5)
                .strong(),
        );
        for line in since_then_lines(&card.since_then, l) {
            ui.label(RichText::new(line).size(11.5));
        }
    });
}

fn short_hash(hash: &str) -> String {
    if hash.len() >= 12 && hash.is_ascii() {
        format!("{}…{}", &hash[..4], &hash[hash.len() - 4..])
    } else {
        hash.to_owned()
    }
}

fn short_date(ms: Option<u64>) -> Option<String> {
    ms.and_then(|ms| i64::try_from(ms / 1000).ok())
        .and_then(|seconds| time::OffsetDateTime::from_unix_timestamp(seconds).ok())
        .map(|date| {
            format!(
                "{:02}/{:02} {:02}:{:02} UTC",
                date.day(),
                u8::from(date.month()),
                date.hour(),
                date.minute()
            )
        })
}

/// "Shop-ready · pt-BR · mm · 24/09 18:12 UTC"
fn meta_line(card: &ReceiptCard, l: &Localizer) -> String {
    let mode = card.mode.map_or_else(
        || l.text("receipt-unknown"),
        |mode| {
            l.text(match mode {
                ExportMode::Draft => "export-draft",
                ExportMode::ShopReady => "export-shop-ready",
            })
        },
    );
    let mut parts = vec![
        mode,
        card.settings.language.tag().to_owned(),
        l.text(unit_key(card.settings.units)),
    ];
    if let Some(date) = short_date(card.completed_unix_ms) {
        parts.push(date);
    }
    parts.join(" · ")
}

fn unit_key(unit: Unit) -> &'static str {
    match unit {
        Unit::Mm => "unit-mm",
        Unit::Cm => "unit-cm",
        Unit::M => "unit-m",
        Unit::Inch => "unit-in",
        Unit::Foot => "unit-ft",
    }
}

/// One short line, never a UUID; the tooltip keeps the complete comparison.
fn since_then_summary(card: &ReceiptCard, l: &Localizer) -> (String, Color32) {
    let since = l.text("receipt-since-then");
    match &card.since_then {
        SinceThen::DetailsUnavailable => (
            format!("{since}: {}", l.text("handoff-since-unavailable")),
            tw::MUTED,
        ),
        SinceThen::NoRecordedChanges => (
            format!("{since}: {}", l.text("handoff-since-none")),
            tw::MUTED,
        ),
        SinceThen::RecordedChanges(changes) => {
            let mut parts: Vec<String> = changes
                .iter()
                .take(2)
                .map(|change| change_summary(change, l))
                .collect();
            if changes.len() > 2 {
                parts.push(l.count("handoff-change-more", (changes.len() - 2) as u64));
            }
            (format!("{since}: {}", parts.join(", ")), STALE_INK)
        }
    }
}

fn change_summary(change: &ComparisonChange, l: &Localizer) -> String {
    let before = change.previous.as_ref();
    let after = change.current.as_ref();
    let name = after
        .or(before)
        .map(|entry| entry.name.trim())
        .filter(|name| !name.is_empty())
        .map_or_else(|| l.text(kind_key(change.kind)), str::to_owned);
    let with_name = |key: &str| {
        let mut args = FluentArgs::new();
        args.set("name", name.clone());
        l.format(key, Some(&args))
    };
    match (before, after) {
        (None, Some(_)) => with_name("handoff-change-added"),
        (Some(_), None) => with_name("handoff-change-removed"),
        (Some(old), Some(new)) => {
            if old.name != new.name && !old.name.is_empty() && !new.name.is_empty() {
                return format!("{} → {}", old.name, new.name);
            }
            for (fact, key) in [
                ("length_um", "handoff-fact-length"),
                ("width_um", "handoff-fact-width"),
                ("thickness_um", "handoff-fact-thickness"),
                ("kerf_um", "handoff-fact-kerf"),
            ] {
                let (Some(a), Some(b)) = (old.facts.get(fact), new.facts.get(fact)) else {
                    continue;
                };
                let (Ok(a), Ok(b)) = (a.parse::<i64>(), b.parse::<i64>()) else {
                    continue;
                };
                if a == b {
                    continue;
                }
                let mut args = FluentArgs::new();
                args.set("name", name.clone());
                args.set("fact", l.text(key));
                args.set(
                    "values",
                    format!("{} → {}", micrometres_mm(a), micrometres_mm(b)),
                );
                return l.format("handoff-change-value", Some(&args));
            }
            with_name("handoff-change-edited")
        }
        (None, None) => with_name("handoff-change-edited"),
    }
}

/// Millimetres with needless zeros trimmed (537, 18.5).
fn micrometres_mm(um: i64) -> String {
    let text = format!("{:.3}", um as f64 / 1000.0);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn labelled(l: &Localizer, key: &str, value: impl std::fmt::Display) -> String {
    format!("{}: {value}", l.text(key))
}

fn freshness(l: &Localizer, status: ReceiptFreshness) -> String {
    l.text(match status {
        ReceiptFreshness::Current => "receipt-current",
        ReceiptFreshness::Outdated => "receipt-outdated",
        ReceiptFreshness::Unavailable => "receipt-unavailable",
        ReceiptFreshness::NotIncluded => "receipt-not-included",
    })
}

fn date_label(ms: Option<u64>, l: &Localizer) -> String {
    ms.and_then(|ms| i64::try_from(ms / 1000).ok())
        .and_then(|seconds| time::OffsetDateTime::from_unix_timestamp(seconds).ok())
        .map(|date| format!("{} {} UTC", date.date(), date.time()))
        .unwrap_or_else(|| l.text("receipt-unavailable"))
}

fn card_lines(card: &ReceiptCard, l: &Localizer) -> Vec<String> {
    let mode = card.mode.map_or_else(
        || l.text("receipt-unknown"),
        |mode| {
            l.text(match mode {
                ExportMode::Draft => "export-draft",
                ExportMode::ShopReady => "export-shop-ready",
            })
        },
    );
    let sections = card.sections.map_or_else(
        || l.text("receipt-unknown"),
        |sections| {
            [
                ("export-section-parts", sections.parts_and_costs),
                ("export-section-sheets", sections.sheets_and_cut_steps),
                ("export-section-hinges", sections.hinge_references),
            ]
            .into_iter()
            .map(|(key, included)| {
                format!(
                    "{}: {}",
                    l.text(key),
                    l.text(if included {
                        "receipt-included"
                    } else {
                        "receipt-omitted"
                    })
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
        },
    );
    let unit = l.text(unit_key(card.settings.units));
    let language = l.text(match card.settings.language {
        Language::En => "language-en",
        Language::PtBr => "language-pt-br",
    });
    vec![
        labelled(l, "receipt-path", card.path.display()),
        labelled(l, "receipt-project-id", card.project_id),
        labelled(l, "pdf-revision", card.revision),
        labelled(l, "receipt-date", date_label(card.completed_unix_ms, l)),
        labelled(l, "receipt-mode", mode),
        labelled(l, "receipt-sections", sections),
        labelled(l, "export-language", language),
        labelled(l, "export-output-units", unit),
        labelled(
            l,
            "receipt-supersession",
            l.text(if card.superseded {
                "receipt-superseded"
            } else {
                "receipt-latest"
            }),
        ),
        labelled(l, "receipt-packet", freshness(l, card.packet)),
        labelled(l, "receipt-wood", freshness(l, card.wood)),
        labelled(
            l,
            "receipt-included-hardware",
            freshness(l, card.included_hardware),
        ),
        labelled(l, "receipt-file-hash", &card.file_sha256),
        labelled(l, "receipt-packet-hash", &card.packet_sha256),
        labelled(l, "receipt-wood-hash", &card.wood_sha256),
    ]
}

fn kind_key(kind: ComparisonKind) -> &'static str {
    match kind {
        ComparisonKind::Project => "receipt-kind-project",
        ComparisonKind::Material => "receipt-kind-material",
        ComparisonKind::Board => "receipt-kind-board",
        ComparisonKind::Stock => "receipt-kind-stock",
        ComparisonKind::Allocation => "receipt-kind-allocation",
        ComparisonKind::Catalog => "receipt-kind-catalog",
        ComparisonKind::Hardware => "receipt-kind-hardware",
        ComparisonKind::Installation => "receipt-kind-installation",
        ComparisonKind::Joint => "receipt-kind-joint",
    }
}

fn change_lines(change: &ComparisonChange, l: &Localizer) -> Vec<String> {
    // Always identify the stable object, including unnamed allocation/installation
    // entries. A missing side is absence, not an inferred zero or default.
    let mut lines = vec![format!("{} · {}", l.text(kind_key(change.kind)), change.id)];
    let before = change.previous.as_ref();
    let after = change.current.as_ref();
    let absent = l.text("receipt-not-present");
    lines.push(format!(
        "{}: {} → {}: {}",
        l.text("receipt-before"),
        before.map_or(absent.as_str(), |entry| entry.name.as_str()),
        l.text("receipt-after"),
        after.map_or(absent.as_str(), |entry| entry.name.as_str()),
    ));
    let keys: BTreeSet<_> = before
        .into_iter()
        .flat_map(|entry| entry.facts.keys())
        .chain(after.into_iter().flat_map(|entry| entry.facts.keys()))
        .collect();
    for key in keys {
        let old = before.and_then(|entry| entry.facts.get(key));
        let new = after.and_then(|entry| entry.facts.get(key));
        if old != new {
            lines.push(format!(
                "{key} · {}: {} → {}: {}",
                l.text("receipt-before"),
                old.map_or(absent.as_str(), String::as_str),
                l.text("receipt-after"),
                new.map_or(absent.as_str(), String::as_str),
            ));
        }
    }
    lines
}

fn since_then_lines(since: &SinceThen, l: &Localizer) -> Vec<String> {
    match since {
        SinceThen::DetailsUnavailable => vec![l.text("receipt-details-unavailable")],
        SinceThen::NoRecordedChanges => vec![l.text("receipt-no-recorded-changes")],
        SinceThen::RecordedChanges(changes) => changes
            .iter()
            .flat_map(|change| change_lines(change, l))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::domain::SrgbColor;
    use plan_my_cabinet::export::{
        ComparisonBaseline, ExportRecord, ExportSettings, ReceiptMetadata, ReceiptSections,
        fingerprint,
    };
    use plan_my_cabinet::persistence::prepare_bytes;
    use std::path::PathBuf;

    fn project() -> Project {
        let mut project = prepare_bytes(include_bytes!(
            "../../tests/fixtures/schema-v1-cabinet.pmcab"
        ))
        .unwrap()
        .project()
        .clone();
        project.export_records.clear();
        project
    }

    fn add_receipt(project: &mut Project, filename: &str, sections: ReceiptSections) {
        let hashes = fingerprint(project);
        project.export_records.push(ExportRecord {
            project_id: project.id,
            revision: project.revision,
            wood_sha256: hashes.wood,
            packet_sha256: hashes.packet,
            settings: ExportSettings {
                language: Language::PtBr,
                units: Unit::Foot,
            },
            path: PathBuf::from(format!("/exports/{filename}")),
            completed_unix_ms: 1_700_000_000_000,
            file_sha256: "a".repeat(64),
            metadata: Box::new(ReceiptMetadata {
                metadata_version: Some(1),
                mode: Some(ExportMode::Draft),
                sections: Some(sections),
                fingerprint_version: Some(6),
                layout_version: Some(1),
                stock_aliases: Some(project.stock_aliases.clone()),
                comparison_baseline: Some(ComparisonBaseline::from_project(project).unwrap()),
            }),
        });
    }

    #[test]
    fn historical_fields_and_supersession_remain_distinct_from_validity_in_both_languages() {
        let mut project = project();
        add_receipt(&mut project, "first.pdf", ReceiptSections::default());
        add_receipt(
            &mut project,
            "second.pdf",
            ReceiptSections {
                hinge_references: false,
                ..ReceiptSections::default()
            },
        );
        for language in [Language::En, Language::PtBr] {
            let l = Localizer::new(language);
            let cards = receipt_cards(&project);
            assert_eq!(cards[0].filename, "second.pdf");
            let newest = card_lines(&cards[0], &l).join("\n");
            let older = card_lines(&cards[1], &l).join("\n");
            assert!(newest.contains(&l.text("receipt-latest")));
            assert!(older.contains(&l.text("receipt-superseded")));
            assert!(older.contains(&l.text("receipt-current")));
            assert!(newest.contains(&l.text("receipt-not-included")));
            assert!(newest.contains("second.pdf"));
            assert!(newest.contains(&"a".repeat(64)));
            assert!(newest.contains("UTC"));
            assert!(newest.contains(&l.text("unit-ft")));
        }
    }

    #[test]
    fn legacy_missing_values_and_unavailable_evidence_are_not_inferred() {
        let mut project = prepare_bytes(include_bytes!(
            "../../tests/fixtures/schema-v1-cabinet.pmcab"
        ))
        .unwrap()
        .project()
        .clone();
        project.export_records[0].completed_unix_ms = 0;
        let card = &receipt_cards(&project)[0];
        let l = Localizer::new(Language::En);
        let lines = card_lines(card, &l).join("\n");
        assert!(lines.contains("Mode: Unknown"));
        assert!(lines.contains("Sections: Unknown"));
        assert!(lines.contains("Date: Unavailable"));
        assert!(
            since_then_lines(&card.since_then, &l)
                .join("\n")
                .contains("unavailable")
        );
    }

    #[test]
    fn supported_changes_show_real_before_after_and_colors_stay_current() {
        let mut edited = project();
        add_receipt(&mut edited, "hinges.pdf", ReceiptSections::default());
        edited.hardware[0].name = "Revised hinge".into();
        let card = &receipt_cards(&edited)[0];
        assert_eq!(card.wood, ReceiptFreshness::Current);
        assert_eq!(card.included_hardware, ReceiptFreshness::Outdated);
        let l = Localizer::new(Language::En);
        let lines = since_then_lines(&card.since_then, &l).join("\n");
        assert!(lines.contains("Pinned hinge"));
        assert!(lines.contains("Revised hinge"));
        assert!(lines.contains(&edited.hardware[0].id.to_string()));

        let mut colors = project();
        add_receipt(&mut colors, "colors.pdf", ReceiptSections::default());
        colors
            .material_colors
            .insert(colors.materials[0].id, SrgbColor([1, 2, 3]));
        colors.revision += 1;
        let card = &receipt_cards(&colors)[0];
        assert_eq!(card.packet, ReceiptFreshness::Current);
        assert_eq!(card.wood, ReceiptFreshness::Current);
        assert_eq!(
            since_then_lines(&card.since_then, &l),
            vec![l.text("receipt-no-recorded-changes")]
        );
    }

    #[test]
    fn headless_render_does_not_edit_the_project() {
        let mut project = project();
        add_receipt(&mut project, "screen.pdf", ReceiptSections::default());
        let original = project.clone();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show_receipts(ui, &project, &Localizer::new(Language::PtBr));
        });
        let labels: Vec<_> = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().or_else(|| node.value()).map(str::to_owned))
            .collect();
        assert!(
            labels.iter().any(|label| label.contains("screen.pdf")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|label| label.contains("Desde então")),
            "{labels:?}"
        );
        output.drop_without_applying_deltas();
        assert_eq!(project, original);
    }

    #[test]
    fn panel_shows_short_cards_without_ids_and_pins_save_note() {
        let mut project = project();
        add_receipt(&mut project, "older.pdf", ReceiptSections::default());
        add_receipt(&mut project, "newer.pdf", ReceiptSections::default());
        project.boards[0].name = "Renamed shelf".into();
        project.revision += 1;
        for language in [Language::En, Language::PtBr] {
            let l = Localizer::new(language);
            let ctx = egui::Context::default();
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(300.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| show_panel(ui, &project, &l, 300.0, 800.0),
            );
            let texts: Vec<(String, egui::Pos2)> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos)),
                    _ => None,
                })
                .collect();
            let all = texts
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                all.contains("newer.pdf") && all.contains("older.pdf"),
                "{all}"
            );
            assert!(all.contains(&l.text("handoff-superseded")), "{all}");
            assert!(!all.contains(&project.id.to_string()), "{all}");
            assert!(!all.contains(&"a".repeat(64)), "full hash leaked: {all}");
            let note = texts
                .iter()
                .find(|(t, _)| {
                    let prefix: String = l.text("handoff-save-note").chars().take(10).collect();
                    t.starts_with(&prefix)
                })
                .unwrap_or_else(|| panic!("save note missing: {all}"));
            assert!(note.1.y > 700.0, "save note not pinned: {note:?}");
            output.drop_without_applying_deltas();
        }
    }

    #[test]
    fn receipt_translation_keys_have_bilingual_parity() {
        fn keys(source: &str) -> BTreeSet<&str> {
            source
                .lines()
                .filter_map(|line| line.split_once(" = "))
                .map(|(key, _)| key)
                .filter(|key| key.starts_with("receipt-"))
                .collect()
        }
        assert_eq!(
            keys(include_str!("../../i18n/en.ftl")),
            keys(include_str!("../../i18n/pt-BR.ftl")),
        );
    }
}
