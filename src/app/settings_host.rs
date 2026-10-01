//! Local preferences, the Settings window and template setup hosting.
use crate::*;

impl DesktopApp {
    /// The desktop host owns the platform path. Capture runs do not call this
    /// initializer and therefore never read or write the user's preferences.
    pub(crate) fn load_preferences(&mut self, ctx: &egui::Context, config_dir: &Path) {
        match PreferencesStore::new(config_dir) {
            Ok(store) => {
                let loaded = store.load();
                self.preferences_error = loaded.warning.map(|error| error.to_string());
                self.preferences = loaded.preferences;
                self.preferences_store = Some(store);
            }
            Err(error) => self.preferences_error = Some(error.to_string()),
        }
        self.localizer.set_language(self.preferences.language);
        ctx.set_zoom_factor(self.preferences.interface_scale.factor());
    }

    pub(crate) fn change_preferences(&mut self, change: impl FnOnce(&mut LocalPreferences)) {
        let mut updated = self.preferences.clone();
        change(&mut updated);
        if updated == self.preferences {
            return;
        }
        self.preferences = updated;
        // An unsuccessful save leaves the change active for this session and
        // visibly reports that it will not survive restart. No project command
        // or revision is involved.
        self.preferences_error = match &self.preferences_store {
            Some(store) => store
                .save(&self.preferences)
                .err()
                .map(|error| error.to_string()),
            None => Some("A platform configuration directory is unavailable".into()),
        };
    }

    pub(crate) fn set_ui_language(&mut self, language: Language) {
        self.change_preferences(|preferences| preferences.language = language);
        self.localizer.set_language(language);
    }

    // These app-local setters are the narrow integration points for General
    // Settings (14.3). Their UI and hint/tint effects are not mounted yet.
    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    pub(crate) fn set_navigation_hints(&mut self, enabled: bool) {
        self.change_preferences(|preferences| preferences.navigation_hints = enabled);
    }

    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    pub(crate) fn set_inverse_scroll_zoom(&mut self, enabled: bool) {
        self.change_preferences(|preferences| preferences.inverse_scroll_zoom = enabled);
    }

    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    pub(crate) fn set_material_tint(&mut self, enabled: bool) {
        self.change_preferences(|preferences| preferences.material_tint = enabled);
    }

    #[allow(dead_code)] // General Settings controls arrive in task 14.3.
    pub(crate) fn set_interface_scale(&mut self, ctx: &egui::Context, scale: InterfaceScale) {
        self.change_preferences(|preferences| preferences.interface_scale = scale);
        ctx.set_zoom_factor(scale.factor());
    }

    pub(crate) fn settings_project_action(&mut self, action: A) {
        self.settings.open = false;
        if self.invoke(Request::new(action)).is_ok() {
            self.settings.resume_after_dialog = true;
        } else {
            self.settings.open = true;
        }
    }

    pub(crate) fn apply_settings_intent(&mut self, ctx: &egui::Context, intent: SettingsIntent) {
        match intent {
            SettingsIntent::Done => self.settings.open = false,
            SettingsIntent::EditKerf => self.settings_project_action(A::EditKerf),
            SettingsIntent::ConfirmKerf => self.settings_project_action(A::ConfirmKerf),
            SettingsIntent::EditGrid => self.settings_project_action(A::EditGrid),
            SettingsIntent::EditCutFee => self.settings_project_action(A::EditCutFee),
            SettingsIntent::ChangeCurrency => self.settings_project_action(A::EditCurrency),
            SettingsIntent::SetDisplayUnit(unit) => self.editor.set_display_unit(unit),
            SettingsIntent::SetFreeCutFee => {
                if let Ok(free) =
                    plan_my_cabinet::money::Money::new(self.editor.project().currency, 0)
                {
                    let _ = self.editor.set_cut_fee(Some(free));
                }
            }
            SettingsIntent::SetLanguage(language) => self.set_ui_language(language),
            SettingsIntent::SetNavigationHints(enabled) => self.set_navigation_hints(enabled),
            SettingsIntent::SetInverseScrollZoom(enabled) => self.set_inverse_scroll_zoom(enabled),
            SettingsIntent::SetMaterialTint(enabled) => self.set_material_tint(enabled),
            SettingsIntent::SetLighting(lighting) => {
                self.change_preferences(|preferences| preferences.lighting = lighting.normalized())
            }
            SettingsIntent::LightFromView => {
                let _ = self.invoke(Request::new(A::LightFromView));
            }
            SettingsIntent::SetScale(scale) => self.set_interface_scale(ctx, scale),
            SettingsIntent::HelpShortcuts => {
                self.settings.state.section = SettingsSection::Shortcuts
            }
            SettingsIntent::Source => ctx.open_url(egui::OpenUrl::new_tab(APP_SOURCE_URL)),
            SettingsIntent::WorkedExamples => {
                self.settings.open = false;
                self.settings.worked_examples = true;
            }
            SettingsIntent::ShowRecoveryFolder => {
                let folder = self
                    .project_files
                    .user_data_dir
                    .clone()
                    .or_else(project_ui::user_data_dir)
                    .map(|root| root.join("recovery"));
                self.settings.message = Some(match folder {
                    Some(folder) if folder.is_dir() => {
                        #[cfg(target_os = "macos")]
                        let command = "open";
                        #[cfg(target_os = "linux")]
                        let command = "xdg-open";
                        #[cfg(target_os = "windows")]
                        let command = "explorer";
                        #[cfg(not(any(
                            target_os = "macos",
                            target_os = "linux",
                            target_os = "windows"
                        )))]
                        let command = "";
                        std::process::Command::new(command)
                            .arg(&folder)
                            .status()
                            .ok()
                            .filter(std::process::ExitStatus::success)
                            .map_or_else(
                                || self.localizer.text("settings-recovery-open-failed"),
                                |_| {
                                    format!(
                                        "{}: {}",
                                        self.localizer.text("settings-recovery-folder"),
                                        folder.display()
                                    )
                                },
                            )
                    }
                    _ => self.localizer.text("settings-recovery-no-folder"),
                });
            }
            SettingsIntent::ReviewRecoveryCleanup => {
                match self
                    .project_files
                    .user_data_dir
                    .clone()
                    .or_else(project_ui::user_data_dir)
                {
                    Some(dir) => match CleanupUi::open(&dir) {
                        Ok(review) => {
                            self.settings.open = false;
                            self.settings.cleanup = Some(review);
                            self.settings.message = None;
                        }
                        Err(error) => self.settings.message = Some(error.to_string()),
                    },
                    None => {
                        self.settings.message =
                            Some(self.localizer.text("settings-recovery-open-failed"))
                    }
                }
            }
        }
    }

    pub(crate) fn show_settings(&mut self, ctx: &egui::Context) {
        if let Some(cleanup) = &mut self.settings.cleanup {
            match cleanup.show(ctx, &self.localizer) {
                CleanupIntent::None => {}
                CleanupIntent::Closed => {
                    self.settings.cleanup = None;
                    self.settings.open = true;
                }
                CleanupIntent::OpenFolder(path) => {
                    #[cfg(target_os = "macos")]
                    let command = "open";
                    #[cfg(target_os = "linux")]
                    let command = "xdg-open";
                    #[cfg(target_os = "windows")]
                    let command = "explorer";
                    #[cfg(not(any(
                        target_os = "macos",
                        target_os = "linux",
                        target_os = "windows"
                    )))]
                    let command = "";
                    if !path.is_dir()
                        || !std::process::Command::new(command)
                            .arg(path)
                            .status()
                            .is_ok_and(|status| status.success())
                    {
                        self.settings.message =
                            Some(self.localizer.text("settings-recovery-open-failed"));
                    }
                }
            }
            return;
        }
        if self.settings.worked_examples {
            let guide = if self.localizer.language() == Language::En {
                include_str!("../../docs/stock-en.md")
            } else {
                include_str!("../../docs/stock-pt-BR.md")
            };
            let excerpt = guide
                .split_once(if self.localizer.language() == Language::En {
                    "## Cutting assumptions and worked examples"
                } else {
                    "## Premissas de corte e exemplos"
                })
                .map_or(guide, |(_, text)| text);
            let chrome = self.settings.examples_chrome.get_or_insert_with(|| {
                modal_chrome::ModalChrome::new(egui::Id::new("settings-worked-examples"))
                    .width(760.0)
            });
            let result = chrome.show_reader(
                ctx,
                &self.localizer.text("settings-worked-examples"),
                &self.localizer.text("settings-back"),
                |ui| {
                    for paragraph in excerpt.split("\n\n") {
                        ui.label(paragraph);
                        ui.add_space(8.0);
                    }
                },
            );
            if result.action == modal_chrome::ModalAction::Cancel {
                chrome.close(ctx);
                self.settings.examples_chrome = None;
                self.settings.worked_examples = false;
                self.settings.open = true;
            }
            return;
        }
        if self.settings.resume_after_dialog
            && !self.other_modal_open()
            && !self.project_files.blocking()
            && self.navigation.pending().is_none()
        {
            self.settings.resume_after_dialog = false;
            self.settings.open = true;
        }
        if self.settings.open {
            let project = (self.capture.is_some() || !self.project_files.welcome.visible)
                .then(|| self.editor.project());
            let estimate = project.and_then(|p| plan_my_cabinet::cost_estimate::estimate(p).ok());
            let intents = self.settings.state.show(
                ctx,
                &self.localizer,
                project,
                &self.preferences,
                estimate.as_ref(),
                self.preferences_error.as_deref(),
                self.settings.message.as_deref(),
            );
            for intent in intents {
                self.apply_settings_intent(ctx, intent);
            }
        }
    }

    pub(crate) fn show_template_setup(&mut self, ctx: &egui::Context) {
        if self.template.guard_pending {
            if self.project_files.prompt.is_none()
                && !self.project_files.blocking()
                && self.navigation.pending().is_none()
                && self.pending_project_command.is_none()
            {
                self.template.guard_pending = false;
            } else {
                return;
            }
        }
        let Some(setup) = &mut self.template.setup else {
            return;
        };
        if let Some(message) = &self.template.message {
            egui::Area::new(egui::Id::new("template-setup-error"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
                .show(ctx, |ui| {
                    ui.colored_label(theme_widgets::WARN_INK, message);
                });
        }
        match setup.show(ctx, &self.localizer) {
            Some(TemplateSetupIntent::Cancel) => {
                self.template.setup = None;
                self.template.message = None;
            }
            Some(TemplateSetupIntent::Accept(accepted)) => {
                setup.setup = accepted;
                self.template.guard_pending = true;
                self.request_project_action(project_ui::NextAction::Template);
            }
            None => {}
        }
    }
}
