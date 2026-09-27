//! Read-only export history for the Handoff workspace. All comparisons come from
//! the receipt read model; this view does not inspect exported files or dispatch actions.

use std::collections::BTreeSet;

use eframe::egui::{self, Color32, RichText};
use plan_my_cabinet::domain::Project;
use plan_my_cabinet::export::{ComparisonChange, ComparisonKind, ExportMode};
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::receipt_read_models::{
    ReceiptCard, ReceiptFreshness, SinceThen, receipt_cards,
};
use plan_my_cabinet::units::Unit;

/// Draws the entire history in recorded newest-first order. The host should
/// call this inside Handoff using its current project and UI-language localizer.
pub fn show_receipts(ui: &mut egui::Ui, project: &Project, l: &Localizer) {
    ui.heading(l.text("export-history"));
    ui.label(l.text("receipt-history-note"));
    let cards = receipt_cards(project);
    if cards.is_empty() {
        ui.label(l.text("receipt-empty"));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("handoff-receipt-history")
        .max_height(420.0)
        .show(ui, |ui| {
            for card in &cards {
                egui::Frame::new()
                    .fill(Color32::from_rgb(251, 250, 247))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(221, 215, 205)))
                    .corner_radius(8)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.heading(&card.filename)
                            .on_hover_text(card.path.display().to_string());
                        for line in card_lines(card, l) {
                            ui.add(egui::Label::new(line).wrap().selectable(true));
                        }
                        ui.separator();
                        ui.label(RichText::new(l.text("receipt-since-then")).strong());
                        for line in since_then_lines(&card.since_then, l) {
                            ui.add(egui::Label::new(line).wrap().selectable(true));
                        }
                    });
                ui.add_space(8.0);
            }
        });
    ui.label(l.text("receipt-save-reminder"));
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
    let unit = l.text(match card.settings.units {
        Unit::Mm => "unit-mm",
        Unit::Cm => "unit-cm",
        Unit::M => "unit-m",
        Unit::Inch => "unit-in",
        Unit::Foot => "unit-ft",
    });
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
        let mut project =
            prepare_bytes(include_bytes!("../tests/fixtures/schema-v1-cabinet.pmcab"))
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
                fingerprint_version: Some(4),
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
        let mut project =
            prepare_bytes(include_bytes!("../tests/fixtures/schema-v1-cabinet.pmcab"))
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
            keys(include_str!("../i18n/en.ftl")),
            keys(include_str!("../i18n/pt-BR.ftl")),
        );
    }
}
