//! Seeing the result: a textual description of the geometry and rendered
//! pictures of the scene and of the stock sheets.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::read_models::scene_description::{DescribeOptions, describe};
use crate::read_models::stock_read_models::StockReadModel;
use crate::render::camera::{Projection, Selection};
use crate::render::picture::{PictureError, PictureRequest, View, render_picture};
use crate::render::raster::RenderError;
use crate::render::sheet::{PartLabels, render_sheet};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::service::workspace::{Kind, Workspace};
use crate::units::Pose;

/// A tool result that carries pictures next to its JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    pub json: Value,
    /// PNG bytes in display order.
    pub images: Vec<Vec<u8>>,
}

impl From<PictureError> for ServiceError {
    fn from(error: PictureError) -> Self {
        match error {
            PictureError::Render(RenderError::EmptyScene) => {
                ServiceError::new(ErrorCode::RenderError, "nothing is visible to draw")
                    .hint("Check the hidden/only lists; the project may have no boards yet.")
            }
            PictureError::Render(RenderError::InvalidSize) => {
                ServiceError::invalid("width and height must be between 64 and 2048")
            }
            PictureError::Render(RenderError::Unprojectable) => {
                ServiceError::new(ErrorCode::RenderError, "the camera cannot see the scene")
            }
            PictureError::Encode(message) => ServiceError::new(ErrorCode::RenderError, message),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct DescribeInput {
    /// Hide these objects (assemblies hide their contents).
    #[serde(default)]
    pub hidden: Vec<String>,
    /// Describe only these objects (and their contents).
    #[serde(default)]
    pub only: Vec<String>,
    /// Faces within this distance count as touching. Default 0.5 mm.
    #[serde(default)]
    pub tolerance_mm: Option<f64>,
    /// Report facing faces up to this far apart as gaps. Default 20 mm.
    #[serde(default)]
    pub gap_report_mm: Option<f64>,
    /// "text" (default: the narrative plus overlaps and gaps) or "full" (also
    /// every object's geometry and every contact).
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewName {
    /// From the front-right, above (default).
    #[default]
    Iso,
    /// From the back-left, above.
    IsoBack,
    /// Looking at the front (-Y side) straight on.
    Front,
    Back,
    Left,
    Right,
    /// Looking down.
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionName {
    #[default]
    Perspective,
    /// No perspective; adds a scale bar. Best for front/top/side views.
    Orthographic,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct RenderViewInput {
    #[serde(default)]
    pub view: ViewName,
    /// Free camera instead of `view`: yaw 0 looks from +X (right side), -90
    /// from the front, 90 from the back; pitch 0 is level, 90 looks down.
    #[serde(default)]
    pub yaw_degrees: Option<f64>,
    #[serde(default)]
    pub pitch_degrees: Option<f64>,
    #[serde(default)]
    pub projection: ProjectionName,
    /// Hide these (assemblies hide their contents), e.g. to see inside.
    #[serde(default)]
    pub hidden: Vec<String>,
    /// Show only these (and their contents); everything else is hidden.
    #[serde(default)]
    pub only: Vec<String>,
    /// Draw these in the selection color.
    #[serde(default)]
    pub highlight: Vec<String>,
    /// Aim and fit the camera at these instead of everything visible.
    #[serde(default)]
    pub frame: Vec<String>,
    /// > 1 zooms in after fitting. Default 1.
    #[serde(default)]
    pub zoom: Option<f64>,
    /// Numbered callouts that match the returned legend. Default true.
    #[serde(default)]
    pub labels: Option<bool>,
    #[serde(default)]
    pub show_grid: bool,
    #[serde(default)]
    pub hide_hardware: bool,
    /// Pixels; default 1024 × 768, at most 2048.
    #[serde(default)]
    pub width: Option<usize>,
    #[serde(default)]
    pub height: Option<usize>,
    /// Also write the PNG to this absolute path.
    #[serde(default)]
    pub save_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct RenderViewsInput {
    /// Up to 6 views, e.g. [{"view":"iso"},{"view":"front","projection":"orthographic"}].
    pub views: Vec<RenderViewInput>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct RenderSheetInput {
    /// Stock piece (alias like "S1", name or id). Omit for every used sheet
    /// (render_sheets).
    #[serde(default)]
    pub stock: Option<String>,
    /// Numbered cut lines. Default true.
    #[serde(default)]
    pub show_cuts: Option<bool>,
    /// "names" (default) or "numbers".
    #[serde(default)]
    pub labels: Option<String>,
    /// Picture width in pixels (default 1100).
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub save_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DoorOpeningInput {
    /// The door (its board name works).
    pub door: String,
    /// Opening angle in degrees (0 = closed).
    pub angle_degrees: f64,
    #[serde(flatten)]
    pub view: RenderViewInput,
}

fn save_png(path: &Option<String>, png: &[u8]) -> ServiceResult<Option<String>> {
    let Some(text) = path else { return Ok(None) };
    let path = PathBuf::from(text);
    if !path.is_absolute() {
        return Err(ServiceError::invalid("save_path must be absolute"));
    }
    std::fs::write(&path, png)?;
    Ok(Some(path.display().to_string()))
}

/// Everything not inside `only` (by id or ancestry), for "show only" views.
fn hidden_except(workspace: &Workspace, only: &HashSet<Uuid>) -> ServiceResult<HashSet<Uuid>> {
    let project = workspace.project()?;
    let mut keep = only.clone();
    // Ancestors of kept objects must stay visible; their other children are hidden instead.
    let parent_of = |id: Uuid| {
        project
            .boards
            .iter()
            .find(|b| b.id == id)
            .and_then(|b| b.parent_id)
            .or_else(|| {
                project
                    .assemblies
                    .iter()
                    .find(|a| a.id == id)
                    .and_then(|a| a.parent_id)
            })
            .or_else(|| {
                project
                    .hardware
                    .iter()
                    .find(|h| h.id == id)
                    .and_then(|h| h.parent_id)
            })
    };
    let mut ancestors = HashSet::new();
    for id in only {
        let mut current = parent_of(*id);
        while let Some(parent) = current {
            if !ancestors.insert(parent) {
                break;
            }
            current = parent_of(parent);
        }
    }
    // Descendants of kept assemblies stay.
    loop {
        let before = keep.len();
        for id in project
            .boards
            .iter()
            .map(|b| (b.id, b.parent_id))
            .chain(project.assemblies.iter().map(|a| (a.id, a.parent_id)))
            .chain(project.hardware.iter().map(|h| (h.id, h.parent_id)))
            .filter(|(_, parent)| parent.is_some_and(|p| keep.contains(&p)))
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            keep.insert(id);
        }
        if keep.len() == before {
            break;
        }
    }
    Ok(project
        .boards
        .iter()
        .map(|b| b.id)
        .chain(project.hardware.iter().map(|h| h.id))
        .chain(project.assemblies.iter().map(|a| a.id))
        .filter(|id| !keep.contains(id) && !ancestors.contains(id))
        .collect())
}

impl Workspace {
    fn visibility(&self, hidden: &[String], only: &[String]) -> ServiceResult<HashSet<Uuid>> {
        let mut out: HashSet<Uuid> = self
            .resolve_all(Kind::Object, hidden)?
            .into_iter()
            .collect();
        if !only.is_empty() {
            let only: HashSet<Uuid> = self.resolve_all(Kind::Object, only)?.into_iter().collect();
            out.extend(hidden_except(self, &only)?);
        }
        Ok(out)
    }

    pub fn describe_scene(&self, input: DescribeInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let selection = Selection {
            hidden: self.visibility(&input.hidden, &input.only)?,
            ..Default::default()
        };
        let options = DescribeOptions {
            tolerance_mm: input.tolerance_mm.unwrap_or(0.5).clamp(0.0, 10.0),
            gap_report_mm: input.gap_report_mm.unwrap_or(20.0).clamp(0.0, 1000.0),
        };
        let description = describe(project, &selection, options);
        let full = input.detail.as_deref() == Some("full");
        Ok(if full {
            let mut value = serde_json::to_value(&description)
                .map_err(|e| ServiceError::internal(e.to_string()))?;
            value["revision"] = json!(project.revision);
            value
        } else {
            json!({
                "revision": project.revision,
                "text": description.text,
                "object_count": description.object_count,
                "overall": description.overall,
                "overlaps": description.overlaps,
                "gaps": description.gaps,
                "floating": description.floating,
            })
        })
    }

    pub(crate) fn picture(
        &self,
        input: &RenderViewInput,
        poses: Option<&HashMap<Uuid, Pose>>,
    ) -> ServiceResult<(Value, Vec<u8>)> {
        let project = self.project()?;
        let view = match (input.yaw_degrees, input.pitch_degrees) {
            (None, None) => match input.view {
                ViewName::Iso => View::Iso,
                ViewName::IsoBack => View::IsoBack,
                ViewName::Front => View::Front,
                ViewName::Back => View::Back,
                ViewName::Left => View::Left,
                ViewName::Right => View::Right,
                ViewName::Top => View::Top,
                ViewName::Bottom => View::Bottom,
            },
            (yaw, pitch) => View::Angles {
                yaw_degrees: yaw.unwrap_or(-45.0),
                pitch_degrees: pitch.unwrap_or(30.0),
            },
        };
        let width = input.width.unwrap_or(1024);
        let height = input.height.unwrap_or(768);
        if !(64..=2048).contains(&width) || !(64..=2048).contains(&height) {
            return Err(ServiceError::invalid(
                "width and height must be between 64 and 2048",
            ));
        }
        let request = PictureRequest {
            view,
            projection: match input.projection {
                ProjectionName::Perspective => Projection::Perspective,
                ProjectionName::Orthographic => Projection::Orthographic,
            },
            width,
            height,
            zoom: input.zoom.unwrap_or(1.0),
            hidden: self.visibility(&input.hidden, &input.only)?,
            highlight: self
                .resolve_all(Kind::Object, &input.highlight)?
                .into_iter()
                .collect(),
            frame: self
                .resolve_all(Kind::Object, &input.frame)?
                .into_iter()
                .collect(),
            labels: input.labels.unwrap_or(true),
            material_tint: true,
            show_hardware: !input.hide_hardware,
            show_grid: input.show_grid,
            poses,
        };
        let picture = render_picture(project, &request)?;
        let saved = save_png(&input.save_path, &picture.png)?;
        let legend: Vec<_> = picture
            .legend
            .iter()
            .filter(|e| e.number.is_some())
            .map(|e| json!({ "n": e.number, "name": e.name, "id": e.id, "kind": e.kind, "visible_percent": e.visible_percent }))
            .collect();
        let not_visible: Vec<_> = picture
            .legend
            .iter()
            .filter(|e| e.number.is_none())
            .map(|e| e.name.clone())
            .collect();
        Ok((
            json!({
                "revision": project.revision,
                "size": [picture.width, picture.height],
                "camera": picture.camera,
                "legend": legend,
                "not_visible_from_here": not_visible,
                "hidden_count": request.hidden.len(),
                "saved_to": saved,
            }),
            picture.png,
        ))
    }

    pub fn render_view(&self, input: RenderViewInput) -> ServiceResult<Rendered> {
        let (json, png) = self.picture(&input, None)?;
        Ok(Rendered {
            json,
            images: vec![png],
        })
    }

    pub fn render_views(&self, input: RenderViewsInput) -> ServiceResult<Rendered> {
        if input.views.is_empty() || input.views.len() > 6 {
            return Err(ServiceError::invalid("give 1 to 6 views"));
        }
        let mut views = Vec::new();
        let mut images = Vec::new();
        for (index, view) in input.views.iter().enumerate() {
            let (mut json, png) = self.picture(view, None)?;
            json["image"] = json!(index + 1);
            views.push(json);
            images.push(png);
        }
        Ok(Rendered {
            json: json!({ "views": views }),
            images,
        })
    }

    pub fn render_sheets(&self, input: RenderSheetInput) -> ServiceResult<Rendered> {
        let project = self.project()?;
        let model = StockReadModel::build(project)
            .map_err(|e| ServiceError::new(ErrorCode::InvalidProject, format!("{e:?}")))?;
        let wanted = input
            .stock
            .as_ref()
            .map(|s| self.resolve(Kind::Stock, s))
            .transpose()?;
        let labels = match input.labels.as_deref() {
            Some("numbers") => PartLabels::Numbers,
            _ => PartLabels::Names,
        };
        let pieces: Vec<_> = model
            .pieces
            .iter()
            .filter(|p| wanted.map_or(p.is_used(), |w| p.id == w))
            .collect();
        if pieces.is_empty() {
            return Err(
                ServiceError::new(ErrorCode::RenderError, "no used sheets to draw")
                    .hint("Place boards on stock first (auto_place)."),
            );
        }
        if pieces.len() > 8 {
            return Err(ServiceError::invalid(format!(
                "{} sheets are used; render them one at a time with stock",
                pieces.len()
            )));
        }
        let width = input.width.unwrap_or(1100).clamp(300, 2400);
        let mut sheets = Vec::new();
        let mut images = Vec::new();
        for piece in pieces {
            let picture = render_sheet(piece, labels, input.show_cuts.unwrap_or(true), width)?;
            let saved = if wanted.is_some() {
                save_png(&input.save_path, &picture.png)?
            } else {
                None
            };
            sheets.push(json!({
                "stock": piece.alias,
                "name": piece.name,
                "material": piece.material_name,
                "status": picture.status,
                "cuts": picture.cut_count,
                "utilization_percent": picture.utilization_percent,
                "parts": picture.legend,
                "saved_to": saved,
            }));
            images.push(picture.png);
        }
        Ok(Rendered {
            json: json!({ "revision": project.revision, "sheets": sheets, "note": "Sheet length runs right, width up; red bands are saw cuts numbered in cutting order; hatched areas are reusable offcuts." }),
            images,
        })
    }

    pub fn render_door_opening(&self, input: DoorOpeningInput) -> ServiceResult<Rendered> {
        let project = self.project()?;
        let id = self.resolve(Kind::Door, &input.door)?;
        let joint = project
            .door_joints
            .iter()
            .find(|j| j.id == id)
            .ok_or_else(|| ServiceError::not_found("door", &input.door))?;
        let poses: HashMap<Uuid, Pose> =
            crate::door_joint::derived_poses(project, joint, input.angle_degrees)?
                .into_iter()
                .collect();
        let (mut json, png) = self.picture(&input.view, Some(&poses))?;
        json["angle_degrees"] = json!(input.angle_degrees);
        json["opening_limit_degrees"] =
            json!(crate::door_joint::opening_limit(project, joint).ok());
        Ok(Rendered {
            json,
            images: vec![png],
        })
    }
}
