//! Explicit, revision-bound shop confirmation; opening is never a project edit.
use crate::*;

pub(super) struct KerfConfirmation {
    project_id: Uuid,
    revision: u64,
    kerf: Length,
    acknowledged: bool,
    failed: bool,
    chrome: ModalChrome,
}

impl KerfConfirmation {
    pub fn new(project: &Project) -> Self {
        Self {
            project_id: project.id,
            revision: project.revision,
            kerf: project.cutting_kerf,
            acknowledged: false,
            failed: false,
            chrome: ModalChrome::new(egui::Id::new("kerf-confirmation"))
                .width(440.0)
                .icon(icons::Icon::Cut),
        }
    }
}

impl DesktopApp {
    pub(super) fn show_kerf_confirmation(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.kerf_confirmation.take() else {
            return;
        };
        let project = self.editor.project();
        let current = project.id == draft.project_id
            && project.revision == draft.revision
            && project.cutting_kerf == draft.kerf;
        let locale = if self.localizer.language() == Language::En {
            Locale::En
        } else {
            Locale::PtBr
        };
        let mut first = None;
        let opening = !draft.chrome.is_active();
        let result = draft.chrome.show(
            ctx,
            &self.localizer.text("kerf-confirm-title"),
            ModalActions {
                cancel: &self.localizer.text("cancel"),
                confirm: &self.localizer.text("kerf-confirm-action"),
            },
            |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.localizer.text("cutting-kerf"),
                    format_length(draft.kerf, Unit::Mm, locale, 3)
                ));
                ui.label(self.localizer.text("cutting-kerf-hint"));
                let response = ui.checkbox(
                    &mut draft.acknowledged,
                    self.localizer.text("kerf-confirm-acknowledge"),
                );
                // egui's checkbox toggles on Enter without consuming the key.
                // Acknowledging is not also permission to submit the parent.
                if response.clicked() {
                    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                }
                first = Some(response.id);
                ui.small(self.localizer.text("kerf-confirm-scope"));
                if !current {
                    ui.colored_label(theme_widgets::DANGER, self.localizer.text("grid-stale"));
                }
                if draft.failed {
                    ui.colored_label(
                        theme_widgets::DANGER,
                        self.localizer.text("cutting-kerf-invalid"),
                    );
                }
                ((), current && draft.acknowledged)
            },
        );
        if opening && let Some(first) = first {
            ctx.memory_mut(|m| m.request_focus(first));
        }
        match result.action {
            ModalAction::Cancel => {
                draft.chrome.close(ctx);
                return;
            }
            ModalAction::Confirm if current && draft.acknowledged => {
                if self.editor.confirm_shop_kerf().is_ok() {
                    draft.chrome.close(ctx);
                    return;
                }
                draft.failed = true;
            }
            _ => {}
        }
        self.kerf_confirmation = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kerf_label_has_one_localized_unit_suffix() {
        for (language, expected) in [(Language::En, "5.000 mm"), (Language::PtBr, "5,000 mm")] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            theme::install_fonts(&ctx);
            let mut app = DesktopApp::default();
            app.localizer.set_language(language);
            app.invoke(Request::new(A::ConfirmKerf)).unwrap();
            let mut output = ctx.run_ui(Default::default(), |_ui| app.show_kerf_confirmation(&ctx));
            output.textures_delta.clear();
            let update = output.platform_output.accesskit_update.as_ref().unwrap();
            let expected = format!("{}: {expected}", app.localizer.text("cutting-kerf"));
            assert!(
                update
                    .nodes
                    .iter()
                    .any(|(_, node)| node.value() == Some(expected.as_str()))
            );
            assert!(
                !update
                    .nodes
                    .iter()
                    .any(|(_, node)| node.value().is_some_and(|text| text.contains("mm mm")))
            );
            output.drop_without_applying_deltas();
        }
    }

    fn key(key: egui::Key) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        }
    }

    fn frame(app: &mut DesktopApp, ctx: &egui::Context, input: egui::RawInput) {
        ctx.run_ui(input, |ui| {
            ui.button("Invoker").on_hover_text("owner");
            app.show_kerf_confirmation(ctx);
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn settings_child_retains_owner_and_returns_after_cancel_without_confirmation() {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut app = DesktopApp::default();
        app.invoke(Request::new(A::OpenSettings)).unwrap();
        app.apply_settings_intent(&ctx, SettingsIntent::ConfirmKerf);
        assert!(!app.settings_open);
        assert!(app.settings_resume_after_dialog);
        assert!(app.kerf_confirmation.is_some());
        let before = app.editor.project().clone();
        let mut invoker = None;
        ctx.run_ui(Default::default(), |ui| {
            let owner = ui.button("Invoker");
            owner.request_focus();
            invoker = Some(owner.id);
            app.show_kerf_confirmation(&ctx);
        })
        .drop_without_applying_deltas();
        assert!(app.action_availability(Request::new(A::NewBoard)).is_err());
        frame(&mut app, &ctx, key(egui::Key::Escape));
        assert_eq!(ctx.memory(|m| m.focused()), invoker);
        ctx.run_ui(Default::default(), |_ui| app.show_settings(&ctx))
            .drop_without_applying_deltas();
        assert!(app.settings_open);
        assert!(!app.settings_resume_after_dialog);
        assert_eq!(app.editor.project(), &before);
    }

    #[test]
    fn explicit_kerf_confirmation_is_cancel_safe_stale_safe_and_undoable() {
        for language in [Language::En, Language::PtBr] {
            let ctx = egui::Context::default();
            theme::install_fonts(&ctx);
            let mut app = DesktopApp::default();
            app.localizer.set_language(language);
            let before = app.editor.project().clone();
            app.invoke(Request::new(A::ConfirmKerf)).unwrap();
            assert!(app.modal_open());
            assert_eq!(app.editor.project(), &before);
            frame(&mut app, &ctx, Default::default());
            // Enter on the acknowledgement must not simultaneously submit.
            frame(&mut app, &ctx, key(egui::Key::Enter));
            assert!(app.kerf_confirmation.is_some());
            assert_eq!(app.editor.project(), &before);
            frame(&mut app, &ctx, key(egui::Key::Escape));
            assert!(app.kerf_confirmation.is_none());
            assert_eq!(app.editor.project(), &before);

            app.invoke(Request::new(A::ConfirmKerf)).unwrap();
            frame(&mut app, &ctx, Default::default());
            app.kerf_confirmation.as_mut().unwrap().acknowledged = true;
            frame(&mut app, &ctx, key(egui::Key::Tab)); // Cancel
            frame(&mut app, &ctx, key(egui::Key::Enter));
            assert!(app.kerf_confirmation.is_none());
            assert_eq!(app.editor.project(), &before);

            app.invoke(Request::new(A::ConfirmKerf)).unwrap();
            frame(&mut app, &ctx, Default::default());
            app.kerf_confirmation.as_mut().unwrap().acknowledged = true;
            frame(&mut app, &ctx, key(egui::Key::Tab));
            frame(&mut app, &ctx, key(egui::Key::Tab)); // Confirm
            frame(&mut app, &ctx, key(egui::Key::Enter));
            assert!(app.kerf_confirmation.is_none());
            assert_eq!(
                app.editor.project().confirmed_shop_kerf,
                Some(before.cutting_kerf)
            );
            assert!(app.editor.project().confirmed_shop_kerf_unix_ms.is_some());
            app.invoke(Request::new(A::Undo)).unwrap();
            assert!(app.editor.project().confirmed_shop_kerf.is_none());
            app.invoke(Request::new(A::Redo)).unwrap();
            assert_eq!(
                app.editor.project().confirmed_shop_kerf,
                Some(before.cutting_kerf)
            );
            app.invoke(Request::new(A::Undo)).unwrap();

            app.invoke(Request::new(A::ConfirmKerf)).unwrap();
            frame(&mut app, &ctx, Default::default());
            app.kerf_confirmation.as_mut().unwrap().acknowledged = true;
            app.editor
                .set_cutting_kerf(Length::from_micrometres(3000))
                .unwrap();
            let changed = app.editor.project().clone();
            frame(&mut app, &ctx, key(egui::Key::Tab));
            frame(&mut app, &ctx, key(egui::Key::Tab));
            frame(&mut app, &ctx, key(egui::Key::Enter));
            assert_eq!(app.editor.project(), &changed);
            assert!(app.editor.project().confirmed_shop_kerf.is_none());
        }
    }
}
