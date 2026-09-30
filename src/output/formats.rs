//! File formats a design can be exported to. The workshop PDF has its own
//! reviewed pipeline (`export`, `workshop_document`, `pdf_export`); every
//! other format is rendered from the format-neutral [`PartList`], so adding
//! one means a variant here, a module under `formats/` and its label.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::machining::MachiningOptions;
use crate::part_list::{PartList, PartListBlocked};

pub mod cortecloud;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportFormat {
    /// The reviewed workshop packet: parts, sheets, cut steps, hardware.
    WorkshopPdf,
    /// A CorteCloud part list, to order cut, banded and drilled parts.
    #[serde(rename = "cortecloud-json")]
    CorteCloudJson,
}

/// What a format needs before it can be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// The Shop-ready review of the Handoff workspace: sheets, cut plan, kerf.
    ShopReadyReview,
    /// Only a structurally valid design with boards: the shop nests the parts.
    ValidDesign,
}

impl ExportFormat {
    pub const ALL: [Self; 2] = [Self::WorkshopPdf, Self::CorteCloudJson];

    /// Stable identifier, used by the agent API.
    pub const fn id(self) -> &'static str {
        match self {
            Self::WorkshopPdf => "workshop-pdf",
            Self::CorteCloudJson => "cortecloud-json",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::WorkshopPdf => "pdf",
            Self::CorteCloudJson => "json",
        }
    }

    /// i18n key of the format's name.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::WorkshopPdf => "export-format-pdf",
            Self::CorteCloudJson => "export-format-cortecloud",
        }
    }

    pub const fn gate(self) -> Gate {
        match self {
            Self::WorkshopPdf => Gate::ShopReadyReview,
            Self::CorteCloudJson => Gate::ValidDesign,
        }
    }

    /// A file name for a project: "Kitchen base-cortecloud.json".
    pub fn file_name(self, project_name: &str) -> String {
        let stem: String = project_name
            .chars()
            .map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c })
            .collect();
        let stem = stem.trim();
        let stem = if stem.is_empty() { "project" } else { stem };
        match self {
            Self::WorkshopPdf => format!("{stem}.pdf"),
            Self::CorteCloudJson => format!("{stem}-cortecloud.json"),
        }
    }
}

/// Settings of a part-list export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FormatOptions {
    pub machining: MachiningOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    Blocked(PartListBlocked),
    /// The PDF is written by the reviewed packet pipeline, not from a part list.
    NotAPartListFormat,
    Encode(String),
}

/// Build the part list and render it in `format`.
pub fn render(
    format: ExportFormat,
    project: &crate::domain::Project,
    options: &FormatOptions,
) -> Result<(PartList, Vec<u8>), FormatError> {
    let list =
        crate::part_list::build(project, &options.machining).map_err(FormatError::Blocked)?;
    let bytes = render_part_list(format, &list)?;
    Ok((list, bytes))
}

pub fn render_part_list(format: ExportFormat, list: &PartList) -> Result<Vec<u8>, FormatError> {
    match format {
        ExportFormat::WorkshopPdf => Err(FormatError::NotAPartListFormat),
        ExportFormat::CorteCloudJson => cortecloud::render(list),
    }
}

/// A file written from a part list. Kept apart from the workshop PDF's
/// receipts, whose freshness rules are about sheets and cut plans.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileExportRecord {
    pub format: ExportFormat,
    pub project_id: Uuid,
    pub revision: u64,
    /// `PartList::fingerprint` of what the file describes.
    pub part_list_sha256: String,
    pub path: std::path::PathBuf,
    pub completed_unix_ms: u64,
    pub file_sha256: String,
}

impl FileExportRecord {
    pub fn is_valid(&self) -> bool {
        let hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        hex(&self.part_list_sha256) && hex(&self.file_sha256)
    }

    /// Whether the file still matches the design, with the same options.
    pub fn is_current(&self, project: &crate::domain::Project, options: &FormatOptions) -> bool {
        crate::part_list::build(project, &options.machining)
            .is_ok_and(|list| list.fingerprint() == self.part_list_sha256)
    }
}

/// Write a part-list format to `destination`, atomically, and return its
/// receipt. The caller records it with `ProjectEditor::record_file_export`.
pub fn write(
    format: ExportFormat,
    project: &crate::domain::Project,
    options: &FormatOptions,
    destination: &std::path::Path,
    overwrite: crate::export::Overwrite,
) -> Result<FileExportRecord, crate::export::OutputError> {
    use crate::export::{OutputError, Overwrite};
    use sha2::{Digest, Sha256};
    if destination.symlink_metadata().is_ok() && overwrite != Overwrite::Confirm {
        return Err(OutputError::OverwriteRequired);
    }
    let (list, bytes) = render(format, project, options).map_err(OutputError::Format)?;
    crate::export::write_file(destination, &bytes, overwrite)?;
    Ok(FileExportRecord {
        format,
        project_id: project.id,
        revision: project.revision,
        part_list_sha256: list.fingerprint(),
        path: destination.to_path_buf(),
        completed_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        file_sha256: format!("{:x}", Sha256::digest(&bytes)),
    })
}
