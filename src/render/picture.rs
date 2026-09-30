//! Labelled PNG pictures of the scene for people and agents: a named or free
//! camera angle, hidden and highlighted objects, numbered callouts that map to
//! a legend, an axis gizmo and (orthographic only) a scale bar.
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use resvg::usvg;
use serde::Serialize;
use uuid::Uuid;

use crate::domain::Project;
use crate::render::camera::{Camera, FOV, Preset, Projection, Selection};
use crate::render::raster::{self, Raster, RenderError, SceneStyle};
use crate::units::Pose;

/// Named camera directions. The cabinet front faces -Y, Z is up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// Front-right-top, the viewport's Iso preset.
    Iso,
    /// Back-left-top.
    IsoBack,
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    /// Viewport orbit angles in degrees: yaw 0 looks from +X, -90 from the front
    /// (-Y); pitch 90 looks straight down.
    Angles {
        yaw_degrees: f64,
        pitch_degrees: f64,
    },
}

impl View {
    pub fn yaw_pitch(self) -> (f64, f64) {
        let iso_pitch = (1.0_f64 / 3.0_f64.sqrt()).asin();
        let d = f64::to_radians;
        match self {
            Self::Iso => (d(-45.0), iso_pitch),
            Self::IsoBack => (d(135.0), iso_pitch),
            Self::Front => (d(-90.0), 0.0),
            Self::Back => (d(90.0), 0.0),
            Self::Right => (0.0, 0.0),
            Self::Left => (d(180.0), 0.0),
            Self::Top => (d(-90.0), d(90.0)),
            Self::Bottom => (d(-90.0), d(-90.0)),
            Self::Angles {
                yaw_degrees,
                pitch_degrees,
            } => (d(yaw_degrees), d(pitch_degrees.clamp(-90.0, 90.0))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PictureRequest<'a> {
    pub view: View,
    pub projection: Projection,
    pub width: usize,
    pub height: usize,
    /// > 1 moves closer after fitting.
    pub zoom: f64,
    /// Hidden objects (assemblies hide their descendants).
    pub hidden: HashSet<Uuid>,
    /// Drawn in the viewport's selection colors.
    pub highlight: HashSet<Uuid>,
    /// Fit the camera to these (and their descendants); empty = all visible.
    pub frame: HashSet<Uuid>,
    pub labels: bool,
    pub material_tint: bool,
    pub show_hardware: bool,
    pub show_grid: bool,
    pub poses: Option<&'a HashMap<Uuid, Pose>>,
}

impl Default for PictureRequest<'_> {
    fn default() -> Self {
        Self {
            view: View::Iso,
            projection: Projection::Perspective,
            width: 1024,
            height: 768,
            zoom: 1.0,
            hidden: HashSet::new(),
            highlight: HashSet::new(),
            frame: HashSet::new(),
            labels: true,
            material_tint: true,
            show_hardware: true,
            show_grid: false,
            poses: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LegendEntry {
    /// The number drawn in the picture; `None` when the object is not visible
    /// from this camera (fully covered or outside the frame).
    pub number: Option<usize>,
    pub id: Uuid,
    pub name: String,
    pub kind: &'static str,
    /// Share of the picture area the object covers, in percent.
    pub visible_percent: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CameraInfo {
    pub yaw_degrees: f64,
    pub pitch_degrees: f64,
    pub distance_mm: f64,
    pub target_mm: [f64; 3],
    pub projection: &'static str,
    /// Millimetres per pixel at the target (orthographic: everywhere).
    pub mm_per_pixel: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub png: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub legend: Vec<LegendEntry>,
    pub camera: CameraInfo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PictureError {
    Render(RenderError),
    Encode(String),
}

impl From<RenderError> for PictureError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

pub fn render_picture(
    project: &Project,
    request: &PictureRequest,
) -> Result<Picture, PictureError> {
    let selection = Selection {
        ids: request.highlight.clone(),
        active: None,
        hidden: request.hidden.clone(),
    };
    let style = SceneStyle {
        width: request.width,
        height: request.height,
        material_tint: request.material_tint,
        show_grid: request.show_grid,
        show_hardware: request.show_hardware,
        show_shadow: request.view.yaw_pitch().1.abs() < 1.2,
        poses: request.poses,
        supersample: 2,
    };
    let (yaw, pitch) = request.view.yaw_pitch();
    let mut camera = Camera {
        projection: request.projection,
        ..Default::default()
    };
    camera.yaw = yaw;
    camera.pitch = pitch;
    camera.preset = Preset::Free;
    let mut points = raster::framing_points(project, &selection, &style, &request.frame);
    if points.is_empty() {
        points = raster::framing_points(project, &selection, &style, &HashSet::new());
    }
    if points.is_empty() {
        return Err(PictureError::Render(RenderError::EmptyScene));
    }
    raster::fit_camera(&mut camera, &points, request.width, request.height, 0.86);
    if request.zoom.is_finite() && request.zoom > 0.0 {
        camera.distance = (camera.distance / request.zoom).clamp(1.0, 1.0e9);
    }
    let scene = raster::render_scene(project, &camera, &selection, &style)?;
    let mm_per_pixel = 2.0 * camera.distance * (FOV / 2.0).tan() / request.height.max(1) as f64;
    let legend = legend(project, &scene);
    let overlay = overlay_svg(&scene, &legend, &camera, request, mm_per_pixel);
    let png = compose(
        scene.rgba.clone(),
        scene.width,
        scene.height,
        overlay.as_deref(),
    )?;
    Ok(Picture {
        png,
        width: scene.width,
        height: scene.height,
        legend,
        camera: CameraInfo {
            yaw_degrees: round3(yaw.to_degrees()),
            pitch_degrees: round3(pitch.to_degrees()),
            distance_mm: round3(camera.distance),
            target_mm: camera.target.map(round3),
            projection: match camera.projection {
                Projection::Perspective => "perspective",
                Projection::Orthographic => "orthographic",
            },
            mm_per_pixel: round3(mm_per_pixel),
        },
    })
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Objects covering fewer pixels than this are listed but not numbered.
const MIN_LABEL_PIXELS: usize = 30;

fn legend(project: &Project, scene: &Raster) -> Vec<LegendEntry> {
    let coverage = scene.coverage();
    let area = (scene.width * scene.height).max(1) as f64;
    let mut number = 0;
    scene
        .objects
        .iter()
        .zip(coverage)
        .map(|(id, (pixels, _))| {
            let (name, kind) = project
                .boards
                .iter()
                .find(|b| b.id == *id)
                .map(|b| (b.name.clone(), "board"))
                .or_else(|| {
                    project.hardware.iter().find(|h| h.id == *id).map(|h| {
                        let kind = if project.foot_spec(h).is_some() {
                            "foot"
                        } else {
                            "hardware"
                        };
                        (h.name.clone(), kind)
                    })
                })
                .or_else(|| {
                    project
                        .slide_installations
                        .iter()
                        .find(|s| s.id == *id)
                        .map(|s| (slide_name(project, s), "slide"))
                })
                .unwrap_or_default();
            let numbered = pixels >= MIN_LABEL_PIXELS;
            if numbered {
                number += 1;
            }
            LegendEntry {
                number: numbered.then_some(number),
                id: *id,
                name,
                kind,
                visible_percent: (pixels as f64 / area * 1000.0).round() / 10.0,
            }
        })
        .collect()
}

/// "Drawer 1 slides (0073.045500SX)".
pub fn slide_name(project: &Project, slide: &crate::domain::SlideInstallation) -> String {
    let drawer = project
        .assemblies
        .iter()
        .find(|a| a.id == slide.drawer_root_id)
        .map(|a| a.name.clone())
        .or_else(|| project.board(slide.drawer_root_id).map(|b| b.name.clone()))
        .unwrap_or_default();
    let code = project
        .catalog
        .iter()
        .find(|c| c.id == slide.catalog_id)
        .map_or("", |c| c.product_id.as_str());
    format!("{drawer} slides ({code})")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn overlay_svg(
    scene: &Raster,
    legend: &[LegendEntry],
    camera: &Camera,
    request: &PictureRequest,
    mm_per_pixel: f64,
) -> Option<String> {
    let (w, h) = (scene.width as f64, scene.height as f64);
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Noto Sans">"#
    );
    // Numbered callouts at the centre of each object's visible pixels, nudged
    // apart so close labels stay readable.
    if request.labels {
        let coverage = scene.coverage();
        let mut placed: Vec<[f64; 2]> = Vec::new();
        for (entry, (_, centre)) in legend.iter().zip(coverage) {
            let Some(number) = entry.number else { continue };
            let mut at = centre;
            for _ in 0..8 {
                let Some(clash) = placed
                    .iter()
                    .find(|p| (p[0] - at[0]).hypot(p[1] - at[1]) < 22.0)
                else {
                    break;
                };
                let (dx, dy) = (at[0] - clash[0], at[1] - clash[1]);
                let len = dx.hypot(dy).max(1e-3);
                let (ux, uy) = if len < 1.0 {
                    (0.0, 1.0)
                } else {
                    (dx / len, dy / len)
                };
                at = [clash[0] + ux * 23.0, clash[1] + uy * 23.0];
            }
            at = [at[0].clamp(12.0, w - 12.0), at[1].clamp(12.0, h - 12.0)];
            if (at[0] - centre[0]).hypot(at[1] - centre[1]) > 2.0 {
                let _ = write!(
                    svg,
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#1f2a30" stroke-width="1.2"/>"##,
                    centre[0], centre[1], at[0], at[1]
                );
            }
            placed.push(at);
            let _ = write!(
                svg,
                r##"<circle cx="{:.1}" cy="{:.1}" r="10" fill="#1f2a30" stroke="#ffffff" stroke-width="1.5"/><text x="{:.1}" y="{:.1}" font-size="11" font-weight="600" fill="#ffffff" text-anchor="middle" dominant-baseline="central">{number}</text>"##,
                at[0],
                at[1],
                at[0],
                at[1] + 0.5
            );
        }
    }
    // Axis gizmo: world X red, Y green, Z blue, like the viewport.
    let (right, up, _) = camera.basis();
    let origin = [44.0, h - 44.0];
    for (axis, color, label) in [
        ([1.0, 0.0, 0.0], "#c4453b", "X"),
        ([0.0, 1.0, 0.0], "#4f9957", "Y"),
        ([0.0, 0.0, 1.0], "#3d70c4", "Z"),
    ] {
        let sx = crate::render::camera::dot(axis, right);
        let sy = -crate::render::camera::dot(axis, up);
        if sx.hypot(sy) < 0.05 {
            // Pointing at or away from the viewer.
            let _ = write!(
                svg,
                r#"<circle cx="{:.1}" cy="{:.1}" r="3" fill="{color}"/><text x="{:.1}" y="{:.1}" font-size="11" fill="{color}">{label}</text>"#,
                origin[0],
                origin[1],
                origin[0] + 5.0,
                origin[1] - 5.0
            );
            continue;
        }
        let end = [origin[0] + sx * 30.0, origin[1] + sy * 30.0];
        let _ = write!(
            svg,
            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{color}" stroke-width="2.5" stroke-linecap="round"/><text x="{:.1}" y="{:.1}" font-size="12" font-weight="600" fill="{color}" text-anchor="middle" dominant-baseline="central">{label}</text>"#,
            origin[0],
            origin[1],
            end[0],
            end[1],
            origin[0] + sx * 40.0,
            origin[1] + sy * 40.0
        );
    }
    // Scale bar where it is exact: orthographic pictures.
    if camera.projection == Projection::Orthographic && mm_per_pixel > 0.0 {
        let target_px = w * 0.18;
        let raw = target_px * mm_per_pixel;
        let magnitude = 10_f64.powf(raw.log10().floor());
        let nice = [1.0, 2.0, 5.0, 10.0]
            .into_iter()
            .map(|m| m * magnitude)
            .rfind(|v| *v <= raw)
            .unwrap_or(magnitude);
        let length = nice / mm_per_pixel;
        let (x, y) = (w - 24.0 - length, h - 24.0);
        let _ = write!(
            svg,
            r##"<path d="M{x:.1} {:.1} V{y:.1} H{:.1} V{:.1}" fill="none" stroke="#1f2a30" stroke-width="2"/><text x="{:.1}" y="{:.1}" font-size="12" fill="#1f2a30" text-anchor="middle">{} mm</text>"##,
            y - 6.0,
            x + length,
            y - 6.0,
            x + length / 2.0,
            y - 10.0,
            nice
        );
    }
    let _ = write!(
        svg,
        r##"<text x="12" y="20" font-size="12" fill="#5b554c">{}</text></svg>"##,
        escape(&caption(request))
    );
    Some(svg)
}

fn caption(request: &PictureRequest) -> String {
    let view = match request.view {
        View::Iso => "Iso".to_owned(),
        View::IsoBack => "Iso back".to_owned(),
        View::Front => "Front".to_owned(),
        View::Back => "Back".to_owned(),
        View::Left => "Left".to_owned(),
        View::Right => "Right".to_owned(),
        View::Top => "Top".to_owned(),
        View::Bottom => "Bottom".to_owned(),
        View::Angles {
            yaw_degrees,
            pitch_degrees,
        } => format!("Yaw {yaw_degrees:.0}°, pitch {pitch_degrees:.0}°"),
    };
    let projection = match request.projection {
        Projection::Perspective => "perspective",
        Projection::Orthographic => "orthographic",
    };
    let hidden = if request.hidden.is_empty() {
        String::new()
    } else {
        format!(", {} hidden", request.hidden.len())
    };
    format!("{view} · {projection}{hidden}")
}

/// The bundled UI fonts, so labels render the same on every machine.
pub fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            for data in [
                &include_bytes!("../../assets/fonts/NotoSans-Regular.ttf")[..],
                &include_bytes!("../../assets/fonts/NotoSans-Medium.ttf")[..],
                &include_bytes!("../../assets/fonts/NotoSans-SemiBold.ttf")[..],
            ] {
                db.load_font_data(data.to_vec());
            }
            db.set_sans_serif_family("Noto Sans");
            Arc::new(db)
        })
        .clone()
}

/// Draw an optional SVG overlay over RGBA pixels and encode a PNG.
pub fn compose(
    rgba: Vec<u8>,
    width: usize,
    height: usize,
    overlay: Option<&str>,
) -> Result<Vec<u8>, PictureError> {
    let size = tiny_skia::IntSize::from_wh(width as u32, height as u32)
        .ok_or_else(|| PictureError::Encode("empty picture".into()))?;
    let mut pixmap = tiny_skia::Pixmap::from_vec(rgba, size)
        .ok_or_else(|| PictureError::Encode("pixel buffer size".into()))?;
    if let Some(svg) = overlay {
        let options = usvg::Options {
            font_family: "Noto Sans".into(),
            fontdb: fonts(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_str(svg, &options)
            .map_err(|e| PictureError::Encode(format!("overlay: {e}")))?;
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
    }
    pixmap
        .encode_png()
        .map_err(|e| PictureError::Encode(e.to_string()))
}

/// Render a standalone SVG document (e.g. a sheet diagram) to PNG, scaled to
/// `width` pixels wide.
pub fn svg_to_png(svg: &str, width: u32) -> Result<(Vec<u8>, u32, u32), PictureError> {
    let options = usvg::Options {
        font_family: "Noto Sans".into(),
        fontdb: fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|e| PictureError::Encode(format!("svg: {e}")))?;
    let size = tree.size();
    let scale = width as f32 / size.width().max(1.0);
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height)
        .ok_or_else(|| PictureError::Encode("empty picture".into()))?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap
        .encode_png()
        .map_err(|e| PictureError::Encode(e.to_string()))?;
    Ok((png, width, height))
}
