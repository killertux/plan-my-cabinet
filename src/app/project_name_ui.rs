//! Setting up a new blank project (name, currency, input unit, like a
//! template's first step), or renaming the open one as one undoable edit.
use crate::*;
use plan_my_cabinet::board_commands::{RenameError, project_name};
use plan_my_cabinet::money::Currency;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NameMode {
    /// Nothing exists yet; Cancel creates nothing.
    Create,
    Rename,
}

pub(crate) struct ProjectNameDialog {
    pub(crate) mode: NameMode,
    pub(crate) text: String,
    /// New projects only; renaming leaves these to their own settings.
    pub(crate) currency: Currency,
    pub(crate) unit: Unit,
    /// Select the whole suggestion once the field has focus.
    select_all: bool,
    chrome: ModalChrome,
}

const FIELD: &str = "project-name-field";

impl ProjectNameDialog {
    pub(crate) fn new(mode: NameMode, text: String, project: &Project) -> Self {
        Self {
            mode,
            text,
            currency: project.currency,
            unit: project.display_unit,
            select_all: true,
            chrome: ModalChrome::new(egui::Id::new("project-name-dialog"))
                .width(420.0)
                .icon(icons::Icon::Folder)
                .first_focus(egui::Id::new(FIELD)),
        }
    }
}

impl DesktopApp {
    /// Test seam: accept the open New project dialog as typed.
    #[cfg(test)]
    pub(crate) fn confirm_new_project(&mut self) {
        let draft = self.modals.take_project_name().expect("new project dialog");
        assert_eq!(draft.mode, NameMode::Create);
        self.create_blank_project(&draft.text, draft.currency, draft.unit);
    }

    pub(crate) fn show_project_name_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.modals.take_project_name() else {
            return;
        };
        let (title, confirm) = match draft.mode {
            NameMode::Create => ("project-new", "project-name-create"),
            NameMode::Rename => ("project-rename-title", "project-name-save"),
        };
        let checked = project_name(&draft.text).map(|_| ());
        let result = draft.chrome.show(
            ctx,
            &self.localizer.text(title),
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &self.localizer.text(confirm),
            },
            |ui| {
                use modal_chrome::form;
                let label = self.localizer.text("project-name-label");
                let response = form::field(ui, &label, |ui| {
                    form::Input::new(egui::Id::new(FIELD), &label, ui.available_width())
                        .text()
                        .invalid(checked.is_err())
                        .show(ui, &mut draft.text)
                });
                if draft.select_all && response.has_focus() {
                    let id = egui::Id::new(FIELD);
                    let mut state =
                        egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(draft.text.chars().count()),
                        )));
                    state.store(ui.ctx(), id);
                    draft.select_all = false;
                }
                match checked {
                    Err(RenameError::TooLong) => {
                        form::error(ui, &self.localizer.text("project-name-too-long"));
                    }
                    Err(_) => {
                        form::hint(ui, &self.localizer.text("project-name-empty"));
                    }
                    Ok(_) => {}
                }
                if draft.mode == NameMode::Create {
                    form::gap(ui);
                    form::label(ui, &self.localizer.text("template-setup-currency"));
                    ui.horizontal_wrapped(|ui| {
                        for currency in [Currency::Brl, Currency::Usd] {
                            if ui
                                .selectable_label(draft.currency == currency, currency.code())
                                .clicked()
                            {
                                draft.currency = currency;
                            }
                        }
                    });
                    form::gap(ui);
                    form::label(ui, &self.localizer.text("template-setup-input-unit"));
                    ui.horizontal_wrapped(|ui| {
                        for unit in [Unit::Mm, Unit::Cm, Unit::M, Unit::Inch, Unit::Foot] {
                            if ui
                                .selectable_label(
                                    draft.unit == unit,
                                    crate::template_setup_ui::unit_label(unit),
                                )
                                .clicked()
                            {
                                draft.unit = unit;
                            }
                        }
                    });
                    form::note(ui, &self.localizer.text("template-setup-unit-hint"));
                }
                ((), checked.is_ok())
            },
        );
        match result.action {
            ModalAction::Cancel => draft.chrome.close(ctx),
            ModalAction::Confirm if checked.is_ok() => {
                let name = draft.text.clone();
                draft.chrome.close(ctx);
                match draft.mode {
                    NameMode::Create => {
                        self.create_blank_project(&name, draft.currency, draft.unit)
                    }
                    NameMode::Rename => {
                        let _ = self.editor.rename_project(&name);
                    }
                }
            }
            _ => self.modals.set_project_name(Some(draft)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut DesktopApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.show_project_name_dialog(ui.ctx()),
        )
        .drop_without_applying_deltas();
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Opens the dialog, lets it focus and select the suggestion, then types.
    fn type_and_enter(app: &mut DesktopApp, ctx: &egui::Context, text: &str) {
        for _ in 0..3 {
            frame(app, ctx, vec![]);
        }
        frame(app, ctx, vec![egui::Event::Text(text.into())]);
        frame(app, ctx, vec![key(egui::Key::Enter)]);
    }

    #[test]
    fn new_project_asks_name_currency_and_unit_and_cancel_creates_nothing() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut app = DesktopApp::default();
        let old = app.editor.project().id;

        app.invoke(Request::new(A::NewProject)).unwrap();
        assert!(app.modals.project_name().is_some());
        assert_eq!(app.editor.project().id, old);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
        assert!(app.modals.project_name().is_none());
        assert_eq!(app.editor.project().id, old);

        app.invoke(Request::new(A::NewProject)).unwrap();
        let draft = app.modals.project_name_mut().unwrap();
        draft.currency = Currency::Usd;
        draft.unit = Unit::Cm;
        type_and_enter(&mut app, &ctx, "Kitchen island");
        assert!(app.modals.project_name().is_none());
        let project = app.editor.project();
        assert_ne!(project.id, old);
        assert_eq!(project.name, "Kitchen island");
        assert_eq!(project.currency, Currency::Usd);
        assert_eq!(project.display_unit, Unit::Cm);
        // A fresh project: naming it is not an undoable edit.
        assert!(!app.editor.can_undo());
    }

    #[test]
    fn rename_replaces_the_name_as_one_undo_step() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut app = DesktopApp::default();
        let original = app.editor.project().name.clone();
        app.invoke(Request::new(A::RenameProject)).unwrap();
        type_and_enter(&mut app, &ctx, "Bathroom vanity");
        assert!(app.modals.project_name().is_none());
        assert_eq!(app.editor.project().name, "Bathroom vanity");
        app.invoke(Request::new(A::Undo)).unwrap();
        assert_eq!(app.editor.project().name, original);
    }
}
