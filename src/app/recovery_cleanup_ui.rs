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
mod tests;
