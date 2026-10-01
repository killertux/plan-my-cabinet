//! Coating in the Design workspace: the inspector's coating section (which
//! face of a one-side coated board is coated, with flip and back to
//! automatic) and the Coating field of the material dialogs.
use crate::actions::{ActionId as A, Argument, Request, Target};
use crate::theme_widgets as tw;
use crate::*;
use plan_my_cabinet::coating::{CoatedFaceEdit, CoatingError, CoatingOutcome};
use plan_my_cabinet::coating_rules::{self, Coated, CoatingState, Facing};
use plan_my_cabinet::domain::{BoardFace, Coating, MaterialKind};

pub(crate) fn coating_key(coating: Coating) -> &'static str {
    match coating {
        Coating::None => "coating-none",
        Coating::OneSide => "coating-one-side",
        Coating::BothSides => "coating-both-sides",
    }
}

fn facing_key(facing: Facing) -> &'static str {
    match facing {
        Facing::Front => "coating-facing-front",
        Facing::Up => "coating-facing-up",
        Facing::Outside => "coating-facing-outside",
        Facing::Right => "coating-facing-right",
    }
}

fn face_key(face: BoardFace) -> &'static str {
    match face {
        BoardFace::MinZ => "coating-face-min",
        BoardFace::MaxZ => "coating-face-max",
    }
}

impl DesktopApp {
    /// Tell the user about boards a coated-face edit left alone.
    pub(crate) fn report_coating(
        &mut self,
        result: Result<CoatingOutcome, plan_my_cabinet::commands::EditError<CoatingError>>,
    ) {
        match result {
            Ok(outcome) if !outcome.skipped.is_empty() => {
                let text = self
                    .localizer
                    .count("coating-skipped", outcome.skipped.len() as u64);
                self.toasts.info(text);
            }
            Ok(_) => {}
            Err(plan_my_cabinet::commands::EditError::Command(CoatingError::NotOneSided)) => {
                let text = self.localizer.text("coating-not-one-sided");
                self.toasts.error(text);
            }
            Err(error) => self.report_edit(Err::<(), _>(error)),
        }
    }

    /// The coating section of the board inspector, for one or more boards.
    /// Hidden when no board is on a material that can be coated.
    pub(crate) fn show_coating_section(&mut self, ui: &mut egui::Ui, boards: &[Uuid]) {
        let project = self.editor.project();
        let states: Vec<(Uuid, CoatingState)> = boards
            .iter()
            .filter(|id| {
                project
                    .board(**id)
                    .and_then(|b| project.material(b.material_id))
                    .is_some_and(|m| m.kind.accepts_coating())
            })
            .filter_map(|id| coating_rules::board_state(project, *id).map(|s| (*id, s)))
            .collect();
        if states.is_empty() {
            return;
        }
        tw::inspector_heading(ui, &self.localizer.text("coating-title"), |_| {});
        let summary = match states.as_slice() {
            [(_, state)] => self.coating_summary(*state),
            many => {
                let one_sided = many.iter().filter(|(_, s)| s.choosable).count();
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("count", one_sided as u64);
                args.set("total", many.len() as u64);
                self.localizer.format("coating-multi", Some(&args))
            }
        };
        ui.label(egui::RichText::new(summary).size(12.0).color(tw::TEXT_2));
        let choosable: Vec<&(Uuid, CoatingState)> =
            states.iter().filter(|(_, s)| s.choosable).collect();
        if choosable.is_empty() {
            return;
        }
        let target = if boards.len() == 1 {
            Target::Board(boards[0])
        } else {
            Target::None
        };
        let manual = choosable.iter().any(|(_, s)| s.automatic.is_none());
        let enabled = !self.modal_open();
        let mut pending = None;
        ui.horizontal(|ui| {
            if tw::text_button(
                ui,
                &self.localizer.text("coating-flip"),
                tw::ACCENT_DARK,
                enabled,
            )
            .clicked()
            {
                pending = Some(CoatedFaceEdit::Flip);
            }
            if manual
                && tw::text_button(
                    ui,
                    &self.localizer.text("coating-auto"),
                    tw::ACCENT_DARK,
                    enabled,
                )
                .clicked()
            {
                pending = Some(CoatedFaceEdit::Auto);
            }
        });
        if let Some(edit) = pending {
            self.invoke_or_report(
                Request::with(A::SetCoatedFace, target).argument(Argument::CoatedFace(edit)),
            );
        }
    }

    fn coating_summary(&self, state: CoatingState) -> String {
        match state.coated {
            Coated::Raw => self.localizer.text("coating-summary-raw"),
            Coated::Both => self.localizer.text("coating-summary-both"),
            Coated::One(face) => {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("face", self.localizer.text(face_key(face)));
                match state.automatic {
                    Some(facing) => {
                        args.set("facing", self.localizer.text(facing_key(facing)));
                        self.localizer.format("coating-summary-auto", Some(&args))
                    }
                    None => self.localizer.format("coating-summary-manual", Some(&args)),
                }
            }
        }
    }
}

/// The Coating choice of the material dialogs, for kinds sold coated.
/// Returns whether it changed.
pub(crate) fn material_coating_field(
    ui: &mut egui::Ui,
    localizer: &Localizer,
    salt: &str,
    kind: MaterialKind,
    coating: &mut Coating,
) -> bool {
    use crate::modal_chrome::form;
    if !kind.accepts_coating() {
        return false;
    }
    let before = *coating;
    let width = ui.available_width();
    form::gap(ui);
    form::field(ui, &localizer.text("material-coating"), |ui| {
        egui::ComboBox::from_id_salt((salt, "coating"))
            .width(width)
            .selected_text(localizer.text(coating_key(*coating)))
            .show_ui(ui, |ui| {
                for option in Coating::ALL {
                    ui.selectable_value(coating, option, localizer.text(coating_key(option)));
                }
            });
    });
    ui.label(
        egui::RichText::new(localizer.text("material-coating-hint"))
            .size(11.5)
            .color(tw::FAINT),
    );
    before != *coating
}
