//! Explicit, session-only recovery cleanup. The host owns the native folder
//! opener and keeps this controller alive while its modal is visible.
use std::path::{Path, PathBuf};

use eframe::egui::{self, Id, RichText};
use plan_my_cabinet::i18n::Localizer;
use plan_my_cabinet::recovery::{
    CleanupOutcome, DiscoveryStatus, PendingRecoveryCleanup, RecoveryCleanupReview, RecoveryError,
    RecoveryIdentity, RecoveryIndex,
};

use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};
use crate::theme_widgets;

#[derive(Debug, Eq, PartialEq)]
pub enum CleanupIntent {
    None,
    Closed,
    /// Open this path with the platform file manager; do not treat it as a
    /// project directory or create it just to display it.
    OpenFolder(PathBuf),
}

pub struct CleanupUi {
    index: RecoveryIndex,
    review: RecoveryCleanupReview,
    pending: Option<PendingRecoveryCleanup>,
    review_chrome: ModalChrome,
    confirm_chrome: ModalChrome,
    pub error: Option<String>,
    pub outcome: Option<CleanupOutcome>,
}

impl CleanupUi {
    /// `data_dir` is the application's platform user-data directory. Each open
    /// starts a fresh review with no preselected snapshots, including invalid
    /// ones; discovery never scans an arbitrary user folder.
    pub fn open(data_dir: &Path) -> Result<Self, RecoveryError> {
        let index = RecoveryIndex::open(data_dir)?;
        let review = index.cleanup_review();
        Ok(Self {
            index,
            review,
            pending: None,
            review_chrome: ModalChrome::new(Id::new("recovery-cleanup-review")).width(620.0),
            confirm_chrome: ModalChrome::new(Id::new("recovery-cleanup-confirm")).width(540.0),
            error: None,
            outcome: None,
        })
    }

    #[cfg(test)]
    pub fn review(&self) -> &RecoveryCleanupReview {
        &self.review
    }

    #[cfg(test)]
    pub fn pending(&self) -> Option<&PendingRecoveryCleanup> {
        self.pending.as_ref()
    }

    /// A row's registered identity AND snapshot path are required. The saved
    /// project path is shown for context but cannot be passed as a selection.
    pub fn set_selected(
        &mut self,
        identity: &RecoveryIdentity,
        snapshot_path: &Path,
        selected: bool,
    ) -> Result<(), RecoveryError> {
        if selected {
            self.review.select(identity, snapshot_path)
        } else {
            self.review.deselect(identity, snapshot_path);
            Ok(())
        }
    }

    pub fn begin_confirmation(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let review = std::mem::replace(&mut self.review, self.index.cleanup_review());
        self.pending = review.prepare_confirmation();
        self.pending.is_some()
    }

    /// Drop the opaque token. No filesystem operation is performed on cancel.
    /// Returning to a fresh, unselected review requires explicit selection again.
    pub fn cancel_confirmation(&mut self) {
        self.pending = None;
        self.review = self.index.cleanup_review();
    }

    /// Only call on the affirmative Delete action. Even if one target changed
    /// after selection, the core processes the others and reports that failure.
    pub fn confirm_deletion(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let token = pending.confirmation_token();
        match pending.confirm(&self.index, token) {
            Ok(outcome) => {
                self.error = None;
                self.outcome = Some(outcome);
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.outcome = None;
            }
        }
        self.review = self.index.cleanup_review();
    }

    /// Render one isolated modal per frame. Host code should suppress raw
    /// project/scene shortcuts while this controller is active. `Closed` means
    /// drop it; `OpenFolder` asks the host to use its platform file manager.
    pub fn show(&mut self, ctx: &egui::Context, l: &Localizer) -> CleanupIntent {
        if let Some(pending) = &self.pending {
            let result = self.confirm_chrome.show(
                ctx,
                &l.text("recovery-cleanup-confirm-title"),
                ModalActions {
                    cancel: &l.text("cancel"),
                    confirm: &l.text("recovery-cleanup-delete"),
                },
                |ui| {
                    ui.label(l.text("recovery-cleanup-confirm-detail"));
                    for (identity, path) in pending.selected() {
                        ui.separator();
                        ui.label(RichText::new(identity.project_id.to_string()).strong());
                        ui.add(
                            egui::Label::new(path.display().to_string())
                                .wrap()
                                .selectable(true),
                        );
                    }
                    ((), true)
                },
            );
            match result.action {
                ModalAction::None => CleanupIntent::None,
                ModalAction::Cancel => {
                    self.confirm_chrome.close(ctx);
                    self.cancel_confirmation();
                    CleanupIntent::None
                }
                ModalAction::Confirm => {
                    self.confirm_chrome.close(ctx);
                    self.confirm_deletion();
                    CleanupIntent::None
                }
            }
        } else {
            let selected: Vec<_> = self
                .review
                .selected()
                .map(|(identity, path)| (identity.clone(), path.to_path_buf()))
                .collect();
            let result = self.review_chrome.show(
                ctx,
                &l.text("recovery-cleanup-title"),
                ModalActions {
                    cancel: &l.text("cancel"),
                    confirm: &l.text("recovery-cleanup-review-delete"),
                },
                |ui| {
                    ui.label(l.text("recovery-cleanup-detail"));
                    let folder = self.index.recovery_folder();
                    ui.add(
                        egui::Label::new(folder.display().to_string())
                            .wrap()
                            .selectable(true),
                    );
                    let open_folder = ui.button(l.text("recovery-cleanup-open-folder")).clicked();
                    ui.separator();
                    if self.review.rows().is_empty() {
                        ui.label(l.text("recovery-cleanup-empty"));
                    }
                    let mut toggles = Vec::new();
                    for row in self.review.rows() {
                        let is_selected = selected.iter().any(|(identity, path)| {
                            identity == &row.identity && path == &row.snapshot_path
                        });
                        let mut checked = is_selected;
                        egui::Frame::new()
                            .stroke(egui::Stroke::new(1.0, theme_widgets::BORDER))
                            .inner_margin(10)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if ui
                                    .checkbox(&mut checked, l.text("recovery-cleanup-select"))
                                    .changed()
                                {
                                    toggles.push((
                                        row.identity.clone(),
                                        row.snapshot_path.clone(),
                                        checked,
                                    ));
                                }
                                ui.label(format!(
                                    "{}: {}",
                                    l.text("recovery-cleanup-project-id"),
                                    row.identity.project_id
                                ));
                                let saved = row.identity.saved_path.as_ref().map_or_else(
                                    || l.text("recovery-cleanup-untitled"),
                                    |path| path.display().to_string(),
                                );
                                ui.add(
                                    egui::Label::new(format!(
                                        "{}: {saved}",
                                        l.text("recovery-cleanup-project-path")
                                    ))
                                    .wrap()
                                    .selectable(true),
                                );
                                ui.add(
                                    egui::Label::new(format!(
                                        "{}: {}",
                                        l.text("recovery-cleanup-snapshot"),
                                        row.snapshot_path.display()
                                    ))
                                    .wrap()
                                    .selectable(true),
                                );
                                let status = match &row.status {
                                    DiscoveryStatus::Newer => l.text("recovery-cleanup-newer"),
                                    DiscoveryStatus::NotNewer => {
                                        l.text("recovery-cleanup-not-newer")
                                    }
                                    DiscoveryStatus::Untitled => {
                                        l.text("recovery-cleanup-untitled")
                                    }
                                    DiscoveryStatus::SavedUnavailable(reason) => format!(
                                        "{}: {reason}",
                                        l.text("recovery-cleanup-saved-unavailable")
                                    ),
                                    DiscoveryStatus::Invalid(reason) => {
                                        format!("{}: {reason}", l.text("recovery-cleanup-invalid"))
                                    }
                                };
                                ui.add(
                                    egui::Label::new(format!(
                                        "{}: {status}",
                                        l.text("recovery-cleanup-status")
                                    ))
                                    .wrap(),
                                );
                                let modified = row
                                    .snapshot_modified
                                    .and_then(|date| {
                                        date.duration_since(std::time::UNIX_EPOCH)
                                            .ok()
                                            .and_then(|elapsed| {
                                                i64::try_from(elapsed.as_secs()).ok()
                                            })
                                            .and_then(|seconds| {
                                                time::OffsetDateTime::from_unix_timestamp(seconds)
                                                    .ok()
                                            })
                                            .map(|date| {
                                                format!("{} {} UTC", date.date(), date.time())
                                            })
                                    })
                                    .unwrap_or_else(|| l.text("recovery-cleanup-unknown"));
                                ui.label(format!(
                                    "{}: {modified}",
                                    l.text("recovery-cleanup-modified")
                                ));
                            });
                        ui.add_space(6.0);
                    }
                    if let Some(outcome) = &self.outcome {
                        ui.separator();
                        ui.label(format!(
                            "{}: {}",
                            l.text("recovery-cleanup-deleted"),
                            outcome.deleted.len()
                        ));
                        for failure in &outcome.failed {
                            ui.colored_label(
                                theme_widgets::WARN_INK,
                                format!(
                                    "{}: {} — {}",
                                    l.text("recovery-cleanup-failed"),
                                    failure.snapshot_path.display(),
                                    failure.error
                                ),
                            );
                        }
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(theme_widgets::WARN_INK, error);
                    }
                    ((toggles, open_folder), !selected.is_empty())
                },
            );
            for (identity, path, checked) in result.body.0 {
                if let Err(error) = self.set_selected(&identity, &path, checked) {
                    self.error = Some(error.to_string());
                } else {
                    self.error = None;
                }
            }
            if result.body.1 {
                return CleanupIntent::OpenFolder(self.index.recovery_folder());
            }
            match result.action {
                ModalAction::None => CleanupIntent::None,
                ModalAction::Cancel => {
                    self.review_chrome.close(ctx);
                    CleanupIntent::Closed
                }
                ModalAction::Confirm => {
                    // Suspend the review controller, retaining its last focus
                    // until the child confirmation returns. Do not restore the
                    // Settings invoker while cleanup is still open.
                    self.begin_confirmation();
                    CleanupIntent::None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plan_my_cabinet::commands::ProjectEditor;
    use plan_my_cabinet::domain::Project;
    use plan_my_cabinet::i18n::Language;
    use plan_my_cabinet::money::Currency;
    use plan_my_cabinet::persistence;
    use plan_my_cabinet::recovery::{AUTOSAVE_DELAY, RecoveryStore};
    use std::{fs, time::Instant};
    use uuid::Uuid;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("pmcab-cleanup-ui-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn saved(index: &mut RecoveryIndex, data: &Path, file: &Path) -> PathBuf {
        let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Usd)).unwrap();
        persistence::save(&mut editor, file).unwrap();
        index.register_saved(file, editor.project().id).unwrap();
        editor
            .transact(|p| -> Result<(), ()> {
                p.name = "Draft".into();
                Ok(())
            })
            .unwrap();
        let mut store = RecoveryStore::new(data, file, editor.project().id).unwrap();
        let now = Instant::now();
        store.note_committed_edit(&editor, now).unwrap();
        store.tick(&editor, now + AUTOSAVE_DELAY).unwrap();
        store.recovery_path().to_path_buf()
    }

    #[test]
    fn cancellation_and_partial_failure_preserve_unselected_and_saved_bytes() {
        let temp = Directory::new();
        let data = temp.0.join("data");
        let mut index = RecoveryIndex::open(&data).unwrap();
        let files: Vec<_> = (0..3).map(|n| temp.0.join(format!("{n}.pmcab"))).collect();
        let snapshots: Vec<_> = files
            .iter()
            .map(|file| saved(&mut index, &data, file))
            .collect();
        let saved_bytes: Vec<_> = files.iter().map(|file| fs::read(file).unwrap()).collect();
        let mut ui = CleanupUi::open(&data).unwrap();
        assert_eq!(ui.review().selected().count(), 0);
        assert!(!ui.begin_confirmation());
        let rows: Vec<_> = ui
            .review()
            .rows()
            .iter()
            .take(2)
            .map(|row| (row.identity.clone(), row.snapshot_path.clone()))
            .collect();
        assert!(ui.set_selected(&rows[0].0, &files[0], true).is_err());
        for (identity, path) in &rows {
            ui.set_selected(identity, path, true).unwrap();
        }
        ui.set_selected(&rows[1].0, &rows[1].1, false).unwrap();
        assert_eq!(ui.review().selected().count(), 1);
        ui.set_selected(&rows[1].0, &rows[1].1, true).unwrap();
        assert!(ui.begin_confirmation());
        assert_eq!(ui.pending().unwrap().selected().count(), 2);
        ui.cancel_confirmation();
        assert_eq!(ui.review().selected().count(), 0);
        assert!(snapshots.iter().all(|p| p.exists()));
        for (identity, path) in &rows {
            ui.set_selected(identity, path, true).unwrap();
        }
        assert!(ui.begin_confirmation());
        fs::write(&snapshots[0], b"changed since selection").unwrap();
        ui.confirm_deletion();
        let outcome = ui.outcome.as_ref().unwrap();
        assert_eq!(outcome.deleted, [snapshots[1].clone()]);
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].snapshot_path, snapshots[0]);
        assert!(snapshots[0].exists() && snapshots[2].exists());
        assert!(!snapshots[1].exists());
        for (file, bytes) in files.iter().zip(saved_bytes) {
            assert_eq!(fs::read(file).unwrap(), bytes);
        }
        assert_eq!(ui.review().selected().count(), 0);
    }

    #[test]
    fn headless_modal_displays_invalid_status_identity_and_paths_in_both_languages() {
        let temp = Directory::new();
        let data = temp.0.join("data");
        let file = temp.0.join("saved.pmcab");
        let mut index = RecoveryIndex::open(&data).unwrap();
        let snapshot = saved(&mut index, &data, &file);
        fs::write(&snapshot, b"invalid snapshot").unwrap();
        for language in [Language::En, Language::PtBr] {
            let mut ui = CleanupUi::open(&data).unwrap();
            assert!(matches!(
                ui.review().rows()[0].status,
                DiscoveryStatus::Invalid(_)
            ));
            let ctx = egui::Context::default();
            crate::theme::install_fonts(&ctx);
            ctx.enable_accesskit();
            let mut output = ctx.run_ui(egui::RawInput::default(), |screen| {
                assert_eq!(
                    ui.show(screen.ctx(), &Localizer::new(language)),
                    CleanupIntent::None
                );
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
            for expected in [
                file.display().to_string(),
                snapshot.display().to_string(),
                ui.review().rows()[0].identity.project_id.to_string(),
                Localizer::new(language).text("recovery-cleanup-invalid"),
            ] {
                assert!(
                    labels.iter().any(|label| label.contains(&expected)),
                    "missing {expected}: {labels:?}"
                );
            }
            output.textures_delta.clear();
        }
        assert!(snapshot.exists() && file.exists());
    }

    #[test]
    fn focused_cancel_never_deletes_and_confirm_requires_deliberate_activation() {
        let temp = Directory::new();
        let data = temp.0.join("data");
        let file = temp.0.join("saved.pmcab");
        let mut index = RecoveryIndex::open(&data).unwrap();
        let snapshot = saved(&mut index, &data, &file);
        let saved_bytes = fs::read(&file).unwrap();
        let mut ui = CleanupUi::open(&data).unwrap();
        let (identity, path) = {
            let row = &ui.review().rows()[0];
            (row.identity.clone(), row.snapshot_path.clone())
        };
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let l = Localizer::new(Language::En);
        let draw = |ctx: &egui::Context, ui: &mut CleanupUi, input: egui::RawInput| {
            let mut intent = CleanupIntent::None;
            let mut output = ctx.run_ui(input, |screen| {
                intent = ui.show(screen.ctx(), &l);
            });
            output.textures_delta.clear();
            intent
        };
        let key = |key| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let release = |key| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        assert_eq!(
            draw(&ctx, &mut ui, egui::RawInput::default()),
            CleanupIntent::None
        );
        // Enter activates the initially focused Cancel, even without selection.
        assert_eq!(
            draw(&ctx, &mut ui, key(egui::Key::Enter)),
            CleanupIntent::Closed
        );
        assert!(ui.pending().is_none() && snapshot.exists());
        draw(&ctx, &mut ui, release(egui::Key::Enter));
        ui.set_selected(&identity, &path, true).unwrap();
        // Selecting a snapshot must not turn keyboard Cancel into confirmation.
        assert_eq!(
            draw(&ctx, &mut ui, key(egui::Key::Enter)),
            CleanupIntent::Closed
        );
        assert!(ui.pending().is_none() && snapshot.exists());
        draw(&ctx, &mut ui, release(egui::Key::Enter));
        // Move deliberately from Cancel to the enabled review action.
        draw(&ctx, &mut ui, key(egui::Key::Tab));
        draw(&ctx, &mut ui, release(egui::Key::Tab));
        draw(&ctx, &mut ui, key(egui::Key::Enter));
        assert!(ui.pending().is_some());
        draw(&ctx, &mut ui, egui::RawInput::default());
        draw(&ctx, &mut ui, release(egui::Key::Enter));
        // The confirmation also starts on Cancel: Enter must preserve bytes.
        draw(&ctx, &mut ui, key(egui::Key::Enter));
        assert!(ui.pending().is_none() && snapshot.exists());
        assert_eq!(fs::read(&file).unwrap(), saved_bytes);
        assert!(ui.review().selected().next().is_none());
        draw(&ctx, &mut ui, release(egui::Key::Enter));
        ui.set_selected(&identity, &path, true).unwrap();
        assert!(ui.begin_confirmation());
        draw(&ctx, &mut ui, egui::RawInput::default());
        draw(&ctx, &mut ui, key(egui::Key::Escape));
        assert!(ui.pending().is_none() && snapshot.exists());
        draw(&ctx, &mut ui, release(egui::Key::Escape));
        ui.set_selected(&identity, &path, true).unwrap();
        assert!(ui.begin_confirmation());
        assert!(ui.pending().is_some());
        draw(&ctx, &mut ui, egui::RawInput::default());
        draw(&ctx, &mut ui, key(egui::Key::Tab));
        draw(&ctx, &mut ui, release(egui::Key::Tab));
        draw(&ctx, &mut ui, key(egui::Key::Enter));
        assert!(!snapshot.exists());
        assert_eq!(ui.outcome.as_ref().unwrap().deleted, [path]);
        assert_eq!(fs::read(&file).unwrap(), saved_bytes);
    }

    #[test]
    fn recovery_cleanup_translations_have_bilingual_key_parity() {
        let keys = |source: &'static str| -> std::collections::BTreeSet<&'static str> {
            source
                .lines()
                .filter_map(|line| line.split_once(" = "))
                .map(|(key, _)| key)
                .filter(|key| key.starts_with("recovery-cleanup-"))
                .collect()
        };
        assert_eq!(
            keys(include_str!("../../i18n/en.ftl")),
            keys(include_str!("../../i18n/pt-BR.ftl"))
        );
    }
}
