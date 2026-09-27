//! Immutable history-card data. A receipt describes bytes already written, not
//! current wood feasibility or permission to export another shop-ready packet.

use std::path::PathBuf;

use crate::domain::Project;
use crate::export::{
    ComparisonChange, ComparisonKind, ExportMode, ExportSettings, ExportStatus, ReceiptSections,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptFreshness {
    Current,
    Outdated,
    Unavailable,
    NotIncluded,
}

/// A structured before/after change is the only source for a named difference.
/// A changed hash alone never supplies a part name or an inferred cause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SinceThen {
    DetailsUnavailable,
    NoRecordedChanges,
    RecordedChanges(Vec<ComparisonChange>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptCard {
    pub path: PathBuf,
    pub filename: String,
    pub project_id: uuid::Uuid,
    pub revision: u64,
    pub completed_unix_ms: Option<u64>,
    pub settings: ExportSettings,
    pub mode: Option<ExportMode>,
    pub sections: Option<ReceiptSections>,
    pub wood_sha256: String,
    pub packet_sha256: String,
    pub file_sha256: String,
    /// A newer receipt exists, independent of whether this file remains current.
    pub superseded: bool,
    pub packet: ReceiptFreshness,
    pub wood: ReceiptFreshness,
    /// Unknown inclusion is distinct from an explicitly omitted hinge section.
    pub included_hardware: ReceiptFreshness,
    pub since_then: SinceThen,
}

fn is_hardware(kind: ComparisonKind) -> bool {
    matches!(
        kind,
        ComparisonKind::Catalog
            | ComparisonKind::Hardware
            | ComparisonKind::Installation
            | ComparisonKind::Joint
    )
}

/// Newest first, preserving each receipt's original settings and evidence.
/// The caller supplies the current in-memory project; no PDF or historical path
/// is opened and no project state is modified.
pub fn receipt_cards(project: &Project) -> Vec<ReceiptCard> {
    let count = project.export_records.len();
    project
        .export_records
        .iter()
        .enumerate()
        .rev()
        .map(|(index, record)| {
            let status = ExportStatus::for_record(project, record);
            let wood = match status {
                ExportStatus::Current | ExportStatus::PacketStale => ReceiptFreshness::Current,
                ExportStatus::WoodStale => ReceiptFreshness::Outdated,
                ExportStatus::Unknown | ExportStatus::NeverExported => {
                    ReceiptFreshness::Unavailable
                }
            };
            let packet = match status {
                ExportStatus::Current => ReceiptFreshness::Current,
                ExportStatus::PacketStale | ExportStatus::WoodStale => ReceiptFreshness::Outdated,
                ExportStatus::Unknown | ExportStatus::NeverExported => {
                    ReceiptFreshness::Unavailable
                }
            };
            let changes = (status != ExportStatus::Unknown)
                .then(|| record.changed_entries(project).ok().flatten())
                .flatten();
            let included_hardware = match record.metadata.sections {
                Some(sections) if !sections.hinge_references => ReceiptFreshness::NotIncluded,
                None => ReceiptFreshness::Unavailable,
                Some(_) => match packet {
                    ReceiptFreshness::Current => ReceiptFreshness::Current,
                    ReceiptFreshness::Unavailable => ReceiptFreshness::Unavailable,
                    ReceiptFreshness::Outdated
                        if changes.as_ref().is_some_and(|changes| {
                            changes.iter().any(|change| is_hardware(change.kind))
                        }) =>
                    {
                        ReceiptFreshness::Outdated
                    }
                    // A packet hash does not isolate hardware from a changed
                    // kerf warning, wood edit, or unrecorded guidance value.
                    ReceiptFreshness::Outdated | ReceiptFreshness::NotIncluded => {
                        ReceiptFreshness::Unavailable
                    }
                },
            };
            let since_then = match changes {
                None => SinceThen::DetailsUnavailable,
                Some(_) if packet == ReceiptFreshness::Current => SinceThen::NoRecordedChanges,
                Some(changes) if changes.is_empty() => SinceThen::DetailsUnavailable,
                Some(changes) => SinceThen::RecordedChanges(changes),
            };
            ReceiptCard {
                path: record.path.clone(),
                filename: record
                    .path
                    .file_name()
                    .unwrap_or(record.path.as_os_str())
                    .to_string_lossy()
                    .into_owned(),
                project_id: record.project_id,
                revision: record.revision,
                completed_unix_ms: record.completed_at(),
                settings: record.settings,
                mode: record.metadata.mode,
                sections: record.metadata.sections,
                wood_sha256: record.wood_sha256.clone(),
                packet_sha256: record.packet_sha256.clone(),
                file_sha256: record.file_sha256.clone(),
                superseded: index + 1 < count,
                packet,
                wood,
                included_hardware,
                since_then,
            }
        })
        .collect()
}
