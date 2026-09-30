//! Native-only verification surface for shared presentation primitives.
//! Opt-in `--capture-gallery`; not a replacement for a product workspace.
use crate::{
    icons::{self, Icon},
    theme, theme_widgets as w,
};
use eframe::egui::{self, Color32, RichText};

pub fn show(ui: &mut egui::Ui) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(w::APP)).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Native components · offline gallery")
                        .font(theme::HEADING.font_id()).color(w::TEXT));
                    ui.label(RichText::new("Reference tokens and real focusable widgets — not a redesigned screen")
                        .font(theme::SMALL.font_id()).color(w::SECONDARY));
                });
            });
            ui.add_space(16.0);
            ui.columns(3, |cols| {
                w::card().show(&mut cols[0], |ui| {
                    w::section_header(ui, "Typography");
                    for (token, sample) in [
                        (theme::HEADING, "Heading · cabinet"),
                        (theme::TITLE, "Title · current project"),
                        (theme::BODY, "Body · ação, dimensões, peça"),
                        (theme::BODY_MEDIUM, "Medium · project stock"),
                        (theme::SECTION, "Section · outliner"),
                        (theme::STATUS, "Status · cutting fees unknown"),
                        (theme::MONO, "764 × 537 × 18 mm"),
                        (theme::MONO_MEDIUM, "S1 / O1 • 35.714%"),
                        (theme::CUT_MARKER, "01 • 02 • 03"),
                        (theme::SHORTCUT, "⌘K / ⌥⇧⌘1"),
                    ] {
                        ui.label(RichText::new(sample).font(token.font_id()));
                    }
                    ui.add_space(16.0);
                    w::section_header(ui, "Palette");
                    for (name, color) in [
                        ("App", w::APP), ("Panel", w::PANEL), ("Canvas", w::VIEWPORT),
                        ("Border", w::BORDER), ("Soft", w::BORDER_SOFT),
                        ("Text", w::TEXT), ("Secondary", w::SECONDARY), ("Muted", w::MUTED),
                        ("Accent", w::ACCENT), ("Danger", w::DANGER),
                    ] {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 18.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 3, color);
                            ui.label(name);
                        });
                    }
                });
                w::card().show(&mut cols[1], |ui| {
                    w::section_header(ui, "Controls");
                    let _ = w::primary_button(ui, "Create board", true);
                    let _ = w::primary_button(ui, "Unavailable", false);
                    let _ = w::secondary_button(ui, "Cancel");
                    ui.horizontal(|ui| {
                        let _ = w::chip(ui, "Selected", w::ACCENT_BG, w::ACCENT_INK);
                        let _ = w::chip(ui, "Owned", w::OK_BG, w::OK_INK);
                        let _ = w::chip(ui, "Warning", w::WARN_BG, w::WARN_INK);
                    });
                    let mut choice = ui.data_mut(|d| *d.get_temp_mut_or_default::<u8>(ui.id().with("choice")));
                    let response = w::segmented(ui, &mut choice, &[(0, "Design"), (1, "Stock"), (2, "Cut plan")]);
                    if response.changed() { ui.data_mut(|d| d.insert_temp(ui.id().with("choice"), choice)); }
                    ui.add_space(8.0);
                    let field_id = ui.id().with("gallery-length");
                    let mut text = ui.data_mut(|d| d.get_temp::<String>(field_id).unwrap_or_else(|| "764.000".into()));
                    let response = w::unit_field(ui, field_id, "Board length", &mut text, "mm", None);
                    if response.changed() { ui.data_mut(|d| d.insert_temp(field_id, text)); }
                    let invalid_id = ui.id().with("gallery-invalid");
                    let mut invalid = ui.data_mut(|d| d.get_temp::<String>(invalid_id).unwrap_or_else(|| "not a number".into()));
                    let response = w::unit_field(ui, invalid_id, "Invalid width", &mut invalid, "mm", Some("Enter a positive dimension"));
                    if response.changed() { ui.data_mut(|d| d.insert_temp(invalid_id, invalid)); }
                    ui.add_space(8.0);
                    w::section_header(ui, "Focusable icons");
                    ui.horizontal_wrapped(|ui| {
                        for (icon, label, color, selected, enabled) in [
                            (Icon::Save, "Save project", w::SECONDARY, false, true),
                            (Icon::Move, "Move board", w::ACCENT, true, true),
                            (Icon::Warning, "Warnings", w::WARN_INK, false, true),
                            (Icon::Trash, "Delete board", w::MUTED, false, false),
                        ] {
                            let _ = w::icon_button(ui, icons::icon(icon, color, 15.0).alt_text(label), label, selected, enabled);
                        }
                    });
                });
                w::card().show(&mut cols[2], |ui| {
                    w::section_header(ui, "All 41 bundled icons");
                    for chunk in Icon::ALL.chunks(4) {
                        ui.horizontal(|ui| {
                            for &symbol in chunk {
                                ui.vertical(|ui| {
                                    ui.set_width(69.0);
                                    let _ = w::icon_button(ui,
                                        icons::icon(symbol, Color32::from_rgb(90,82,72), 19.0)
                                            .alt_text(symbol.name()), symbol.name(), false, true);
                                    ui.label(RichText::new(symbol.name()).font(theme::RAIL.font_id()));
                                });
                            }
                        });
                    }
                });
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_gallery_controls_are_native_and_offline() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::install_fonts(&ctx);
        icons::install_loaders(&ctx);
        w::apply_visuals(&ctx);
        let mut result = ctx.run_ui(egui::RawInput::default(), show);
        assert!(!result.shapes.is_empty());
        assert_eq!(Icon::ALL.len(), 41);
        let nodes = &result
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes;
        let buttons: Vec<_> = nodes
            .iter()
            .filter(|(_, n)| n.role() == egui::accesskit::Role::Button)
            .map(|(_, n)| (n.label().map(str::to_owned), n.is_disabled()))
            .collect();
        for expected in [
            "Save project",
            "Move board",
            "Warnings",
            "Delete board",
            "Create board",
            "Cancel",
            "Stock",
            "orbit",
            "grain",
        ] {
            assert!(
                buttons
                    .iter()
                    .any(|(label, _)| label.as_deref() == Some(expected)),
                "missing accessible button {expected}"
            );
        }
        assert!(
            buttons
                .iter()
                .any(|(label, disabled)| label.as_deref() == Some("Delete board") && *disabled)
        );
        assert!(
            buttons
                .iter()
                .any(|(label, disabled)| label.as_deref() == Some("Save project") && !disabled)
        );
        result.textures_delta.clear();
    }
}
