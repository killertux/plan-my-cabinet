//! Part-list exports for shops (CorteCloud): a preview of what the file
//! lists and leaves out, and writing it with a receipt. The workshop PDF is
//! reviewed and written from the desktop Handoff workspace.
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::export::Overwrite;
use crate::formats::{self, ExportFormat, FormatOptions};
use crate::machining::{MachiningOptions, OmissionReason, PilotHole};
use crate::part_list::PartListBlocked;
use crate::service::banding::edge_name;
use crate::service::dto::{Change, LengthInput, LengthOut};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::service::workspace::{Workspace, object_name};

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FormatName {
    #[default]
    #[serde(rename = "cortecloud-json")]
    CorteCloudJson,
}

impl From<FormatName> for ExportFormat {
    fn from(name: FormatName) -> Self {
        match name {
            FormatName::CorteCloudJson => Self::CorteCloudJson,
        }
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ScrewPilotInput {
    pub diameter: LengthInput,
    pub depth: LengthInput,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct PartListInput {
    #[serde(default)]
    pub format: FormatName,
    /// Pilot size for screw holes (hinge plates, slides) the catalog does
    /// not size. Without it those holes are left out and listed.
    #[serde(default)]
    pub screw_pilot: Option<ScrewPilotInput>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ExportDesignInput {
    #[serde(default)]
    pub format: FormatName,
    /// Destination file, e.g. "/Users/me/Desktop/kitchen-cortecloud.json".
    pub path: String,
    /// Replace an existing file.
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub screw_pilot: Option<ScrewPilotInput>,
}

fn options(pilot: Option<&ScrewPilotInput>) -> ServiceResult<FormatOptions> {
    let screw_pilot = pilot
        .map(|p| -> ServiceResult<PilotHole> {
            Ok(PilotHole {
                diameter: p.diameter.positive("screw_pilot.diameter", true)?,
                depth: p.depth.positive("screw_pilot.depth", true)?,
            })
        })
        .transpose()?;
    Ok(FormatOptions {
        machining: MachiningOptions { screw_pilot },
    })
}

impl From<PartListBlocked> for ServiceError {
    fn from(blocked: PartListBlocked) -> Self {
        match blocked {
            PartListBlocked::NoBoards => {
                ServiceError::new(ErrorCode::InvalidArgument, "the design has no boards")
            }
            PartListBlocked::InvalidDesign(error) => error.into(),
        }
    }
}

impl Workspace {
    pub fn get_part_list(&self, input: PartListInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let unit = project.display_unit;
        let options = options(input.screw_pilot.as_ref())?;
        let list = crate::part_list::build(project, &options.machining)?;
        let file = formats::cortecloud::file(&list);
        let parts: Vec<Value> = list
            .groups
            .iter()
            .zip(&file.parts)
            .map(|(group, part)| {
                let banding: serde_json::Map<String, Value> = crate::domain::BoardEdge::ALL
                    .into_iter()
                    .filter_map(|edge| {
                        group.banding[edge.index()]
                            .as_ref()
                            .map(|b| (edge_name(edge).to_owned(), json!(b.name)))
                    })
                    .collect();
                json!({
                    "quantity": group.quantity(),
                    "name": group.name,
                    "cabinet": group.cabinet,
                    "material": part.material,
                    "length": LengthOut::new(group.length, unit),
                    "width": LengthOut::new(group.width, unit),
                    "thickness": LengthOut::new(group.thickness, unit),
                    "banding": banding,
                    "holes": group.machining.face_drills.len(),
                    "boards": group.boards.iter().map(|id| object_name(project, *id)).collect::<Vec<_>>(),
                })
            })
            .collect();
        let left_out: Vec<Value> = list
            .omissions
            .iter()
            .map(|o| {
                json!({
                    "installation": o.installation,
                    "holes": o.holes,
                    "reason": match o.reason {
                        OmissionReason::HasIssues => "the hinge or slides have issues",
                        OmissionReason::DoorNeedsReview => "the door needs review",
                        OmissionReason::PilotSizeUnknown => "no pilot size for these screw holes; pass screw_pilot",
                    },
                })
            })
            .collect();
        Ok(json!({
            "format": ExportFormat::from(input.format).id(),
            "parts": parts,
            "part_count": list.part_count(),
            "left_out": left_out,
            "file_preview": file,
        }))
    }

    pub fn export_design(&mut self, input: ExportDesignInput) -> ServiceResult<Change<Value>> {
        let format = ExportFormat::from(input.format);
        let options = options(input.screw_pilot.as_ref())?;
        let path = std::path::PathBuf::from(&input.path);
        if path.as_os_str().is_empty() {
            return Err(ServiceError::invalid("give a destination path").for_field("path"));
        }
        let overwrite = if input.overwrite {
            Overwrite::Confirm
        } else {
            Overwrite::Decline
        };
        let record = formats::write(format, self.project()?, &options, &path, overwrite).map_err(
            |error| match error {
                crate::export::OutputError::OverwriteRequired => ServiceError::new(
                    ErrorCode::Conflict,
                    "the file exists; pass overwrite: true to replace it",
                ),
                crate::export::OutputError::Format(formats::FormatError::Blocked(blocked)) => {
                    blocked.into()
                }
                other => ServiceError::new(ErrorCode::Io, format!("could not write: {other:?}")),
            },
        )?;
        let summary = format!("Wrote {}.", path.display());
        let result = json!({
            "path": record.path,
            "format": format.id(),
            "file_sha256": record.file_sha256,
        });
        // A receipt is history, not an edit: it does not change the revision.
        self.change(None, |editor| {
            editor.record_file_export(record).map_err(|_| {
                ServiceError::new(ErrorCode::Internal, "could not record the export")
            })?;
            Ok((result, summary))
        })
    }
}
