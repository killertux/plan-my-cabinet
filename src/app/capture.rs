//! Opt-in native baseline capture. This is not a reference-screen pass: the
//! redesigned screens get their own captures when those surfaces are delivered.
use crate::workspace_state::Workspace;
use eframe::egui;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::settings_ui::Section as SettingsSection;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const HELP: &str = "Usage: plan-my-cabinet [--capture-baseline|--capture-gallery NEW_DIRECTORY \
    [--capture-size WIDTHxHEIGHT] [--capture-scale 90|100|115|130] \
    [--capture-language en|pt-BR] [--capture-workspace design|stock|cut-plan|hardware|handoff] [--capture-welcome empty] [--capture-settings cutting|grid|costs|general|shortcuts|about] [--capture-page 1..] [--capture-snap face|grid] [--capture-dialog board|position|face|resize|material|unsaved|palette]]\n\
    Baseline captures a selected application workspace or the isolated empty Welcome; gallery captures offline UI primitives.\n\
    A capture is evidence to review, not automatic redesign acceptance.\n\
    Writes capture.ppm and manifest.json without opening user projects.\n\
    Requires a native graphics session; the output directory must not exist.";

pub type Completion = Arc<Mutex<Option<Result<(), String>>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialog {
    Board,
    Position,
    Face,
    Resize,
    Material,
    Unsaved,
    Palette,
}

impl Dialog {
    pub const ALL: [Self; 7] = [
        Self::Board,
        Self::Position,
        Self::Face,
        Self::Resize,
        Self::Material,
        Self::Unsaved,
        Self::Palette,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Position => "position",
            Self::Face => "face",
            Self::Resize => "resize",
            Self::Material => "material",
            Self::Unsaved => "unsaved",
            Self::Palette => "palette",
        }
    }

    pub fn mount(self, app: &mut crate::DesktopApp) {
        use crate::actions::{ActionId as A, Request, Target};
        use plan_my_cabinet::reference_fixture::{LEFT_SIDE_ID, RIGHT_SIDE_ID};
        app.selection.choose(Some(LEFT_SIDE_ID), false);
        if self == Self::Resize {
            app.selection.choose(Some(RIGHT_SIDE_ID), true);
        }
        if self == Self::Palette {
            app.palette.open(&eframe::egui::Context::default());
            app.palette.query = "sh".into();
        } else if self == Self::Unsaved {
            app.project_files.prompt = Some(crate::project_ui::Prompt::Dirty(
                crate::project_ui::NextAction::New,
            ));
        } else {
            let action = match self {
                Self::Board => A::NewBoard,
                Self::Position => A::PositionBoard,
                Self::Face => A::PlaceFace,
                Self::Resize => A::BatchDimensions,
                Self::Material => A::NewMaterial,
                Self::Unsaved | Self::Palette => unreachable!(),
            };
            let request = if matches!(self, Self::Position | Self::Face) {
                Request::with(action, Target::Board(LEFT_SIDE_ID))
            } else {
                Request::new(action)
            };
            app.invoke(request)
                .expect("reference dialog route is available");
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub directory: PathBuf,
    pub gallery: bool,
    pub size: [u32; 2],
    pub scale: u32,
    pub language: Language,
    pub workspace: Workspace,
    pub welcome_empty: bool,
    pub settings: Option<SettingsSection>,
    pub page: Option<usize>,
    pub snap: Option<crate::viewport::CaptureSnap>,
    pub dialog: Option<Dialog>,
    /// Start from a new project (default materials only) instead of the fixture.
    pub empty_project: bool,
}

impl Config {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Self>, String> {
        let mut args = args.into_iter();
        let Some(first) = args.next() else {
            return Ok(None);
        };
        if first != "--capture-baseline" && first != "--capture-gallery" {
            return Err(HELP.into());
        }
        let directory = args.next().ok_or(HELP)?.into();
        let mut config = Self {
            directory,
            gallery: first == "--capture-gallery",
            size: [1440, 900],
            scale: 100,
            language: Language::En,
            workspace: Workspace::Design,
            welcome_empty: false,
            settings: None,
            page: None,
            snap: None,
            dialog: None,
            empty_project: false,
        };
        let mut seen = std::collections::HashSet::new();
        while let Some(flag) = args.next() {
            let flag = flag.to_str().ok_or(HELP)?;
            if !seen.insert(flag.to_owned()) {
                return Err(format!("Duplicate capture option: {flag}"));
            }
            let value = args.next().ok_or(HELP)?;
            let value = value.to_str().ok_or(HELP)?;
            match flag {
                "--capture-size" => {
                    let (w, h) = value.split_once('x').ok_or(HELP)?;
                    let w = w.parse::<u32>().map_err(|_| HELP)?;
                    let h = h.parse::<u32>().map_err(|_| HELP)?;
                    if !(640..=3840).contains(&w) || !(480..=2160).contains(&h) {
                        return Err(
                            "Capture size must be 640..3840 x 480..2160 logical points".into()
                        );
                    }
                    config.size = [w, h];
                }
                "--capture-scale" => {
                    config.scale = value.parse().map_err(|_| HELP)?;
                    if ![90, 100, 115, 130].contains(&config.scale) {
                        return Err("Capture scale must be 90, 100, 115 or 130".into());
                    }
                }
                "--capture-language" => {
                    config.language = match value {
                        "en" => Language::En,
                        "pt-BR" => Language::PtBr,
                        _ => return Err("Capture language must be en or pt-BR".into()),
                    };
                }
                "--capture-workspace" => {
                    config.workspace = match value {
                        "design" => Workspace::Design,
                        "stock" => Workspace::Stock,
                        "cut-plan" => Workspace::CutPlan,
                        "hardware" => Workspace::Hardware,
                        "handoff" => Workspace::Handoff,
                        _ => return Err("Unknown capture workspace; use design, stock, cut-plan, hardware, or handoff".into()),
                    };
                }
                "--capture-welcome" => {
                    if value != "empty" {
                        return Err("Capture Welcome currently supports only empty".into());
                    }
                    config.welcome_empty = true;
                }
                "--capture-settings" => {
                    config.settings = Some(match value {
                        "cutting" => SettingsSection::Cutting,
                        "grid" => SettingsSection::GridUnits,
                        "costs" => SettingsSection::Costs,
                        "general" => SettingsSection::General,
                        "shortcuts" => SettingsSection::Shortcuts,
                        "about" => SettingsSection::About,
                        _ => return Err("Unknown capture settings section".into()),
                    });
                }
                "--capture-dialog" => {
                    config.dialog = Some(Dialog::ALL.into_iter().find(|dialog| dialog.name() == value)
                        .ok_or("Capture dialog must be board, position, face, resize, material, unsaved or palette")?);
                }
                "--capture-page" => {
                    let page = value
                        .parse::<usize>()
                        .map_err(|_| "Capture page must be a positive number")?;
                    if page == 0 {
                        return Err("Capture page must be a positive number".into());
                    }
                    config.page = Some(page);
                }
                "--capture-project" => {
                    if value != "empty" {
                        return Err("Capture project currently supports only empty".into());
                    }
                    config.empty_project = true;
                }
                "--capture-snap" => {
                    if config.gallery || config.size != [1440, 900] || config.scale != 100 {
                        return Err("Snap capture requires the 1440x900 baseline at 100%".into());
                    }
                    config.snap = Some(match value {
                        "face" => crate::viewport::CaptureSnap::Face,
                        "grid" => crate::viewport::CaptureSnap::Grid,
                        _ => return Err("Capture snap must be face or grid".into()),
                    });
                }
                _ => return Err(format!("Unknown capture option: {flag}\n{HELP}")),
            }
        }
        if config.dialog.is_some()
            && (config.gallery
                || config.welcome_empty
                || config.settings.is_some()
                || config.snap.is_some()
                || config.page.is_some()
                || seen.contains("--capture-workspace"))
        {
            return Err("Dialog capture cannot select another surface".into());
        }
        if config.snap.is_some() && (config.size != [1440, 900] || config.scale != 100) {
            return Err("Snap capture requires the 1440x900 baseline at 100%".into());
        }
        if config.snap.is_some() && config.workspace != Workspace::Design {
            return Err("Snap capture requires the Design workspace".into());
        }
        if config.gallery && config.workspace != Workspace::Design {
            return Err("Gallery capture does not select a workspace".into());
        }
        if config.settings.is_some() && (config.gallery || config.snap.is_some()) {
            return Err("Settings capture cannot also select gallery or snap".into());
        }
        if config.welcome_empty
            && (config.gallery
                || config.snap.is_some()
                || config.settings.is_some()
                || config.page.is_some()
                || seen.contains("--capture-workspace"))
        {
            return Err("Empty Welcome capture cannot also select a workspace, Settings, page, snap, or gallery".into());
        }
        if config.page.is_some()
            && (config.workspace != Workspace::Handoff
                || config.gallery
                || config.settings.is_some())
        {
            return Err("Capture page requires Handoff without Settings or gallery".into());
        }
        Ok(Some(config))
    }

    pub fn prepare_directory(&self) -> io::Result<()> {
        // Never overwrite an existing directory, including a symlink. All local
        // app data for this run lives beneath this exclusively created root.
        fs::create_dir(&self.directory)?;
        fs::create_dir(self.directory.join("app-data"))
    }
}

pub struct Capture {
    config: Config,
    frames: u32,
    completion: Completion,
    fixture_sha256: String,
    snap_evidence: Option<Result<serde_json::Value, String>>,
}

impl Capture {
    pub fn new(config: Config, completion: Completion) -> Self {
        Self {
            config,
            frames: 0,
            completion,
            snap_evidence: None,
            // Hash the normalized project actually shown by DesktopApp, not
            // the pre-editor recipe. Schema migration and alias assignment can
            // enrich an in-memory fixture without changing its geometry.
            fixture_sha256: {
                let fixture = ProjectEditor::new(plan_my_cabinet::reference_fixture::project())
                    .expect("validated reference fixture");
                format!(
                    "{:x}",
                    Sha256::digest(
                        plan_my_cabinet::persistence::serialize(fixture.project())
                            .expect("serializable reference fixture")
                    )
                )
            },
        }
    }

    pub fn gallery(&self) -> bool {
        self.config.gallery
    }

    pub fn welcome_empty(&self) -> bool {
        self.config.welcome_empty
    }

    pub fn set_snap_evidence(&mut self, evidence: Option<Result<serde_json::Value, String>>) {
        self.snap_evidence = evidence;
    }

    pub fn filter_input(&self, input: &mut egui::RawInput) {
        // Keep widgets visually enabled, but allow no keyboard/pointer/drop or
        // accessibility event to invoke file operations while a capture runs.
        input
            .events
            .retain(|e| matches!(e, egui::Event::Screenshot { .. }));
        input
            .events
            .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        input.dropped_files.clear();
        input.hovered_files.clear();
        input.time = Some(f64::from(self.frames) / 60.0);
    }

    /// Returns true when the run has finished and the window should close.
    pub fn tick(&mut self, ctx: &egui::Context) -> bool {
        self.frames += 1;
        let image = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Screenshot {
                    viewport_id, image, ..
                } if *viewport_id == egui::ViewportId::ROOT => Some(image.clone()),
                _ => None,
            })
        });
        let result = if let Some(image) = image {
            Some(self.save(ctx, &image).map_err(|e| e.to_string()))
        } else if self.frames > 300 {
            Some(Err(
                "Native screenshot did not arrive within 300 frames".into()
            ))
        } else {
            None
        };
        if let Some(result) = result {
            *self.completion.lock().expect("capture result mutex") = Some(result);
            return true;
        }
        // Wait for window size, fonts and the GPU scene to settle. The manifest
        // records actual dimensions so OS-constrained window sizes cannot pass.
        // Native screenshot callbacks can be dropped while a Metal surface is
        // being recreated (for example after its initial resize). Re-request
        // at bounded intervals instead of treating the first missed command as
        // a permanent failure; no output is written until a real event arrives.
        if screenshot_request_frame(self.frames) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint();
        false
    }

    fn save(&self, ctx: &egui::Context, image: &egui::ColorImage) -> io::Result<()> {
        let snap = if self.config.snap.is_some() {
            Some(
                self.snap_evidence
                    .as_ref()
                    .ok_or_else(|| io::Error::other("Snap candidate was not prepared"))?
                    .as_ref()
                    .map_err(|error| io::Error::other(error.clone()))?,
            )
        } else {
            None
        };
        let pixels_per_point = ctx.pixels_per_point();
        let actual = image.size.map(|n| n as f32 / pixels_per_point);
        for (actual, requested) in actual.into_iter().zip(self.config.size) {
            if (actual - requested as f32).abs() > 2.0 {
                return Err(io::Error::other(format!(
                    "Native window did not reach requested logical size {:?}; actual {:?}",
                    self.config.size,
                    image.size.map(|n| n as f32 / pixels_per_point)
                )));
            }
        }
        write_ppm(&self.config.directory.join("capture.ppm"), image)?;
        if let Some(evidence) = snap {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.config.directory.join("snap-state.json"))?;
            serde_json::to_writer_pretty(&mut file, evidence)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
        }
        let manifest = serde_json::json!({
            "format_version": 1,
            "surface": if self.config.gallery { "widget-gallery" } else if self.config.welcome_empty { "welcome-empty" } else { "current-application-baseline" },
            "redesign_acceptance": false,
            "application_version": env!("CARGO_PKG_VERSION"),
            "fixture": (!self.config.welcome_empty).then_some("redesign-reference-v1"),
            "fixture_sha256": (!self.config.welcome_empty).then_some(&self.fixture_sha256),
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "selected_board": (!self.config.welcome_empty).then_some(plan_my_cabinet::reference_fixture::SHELF_ID),
            "hidden_assembly": (!self.config.welcome_empty).then_some(plan_my_cabinet::reference_fixture::DOORS_ID),
            "logical_size": self.config.size,
            "pixel_size": image.size,
            "pixels_per_point": pixels_per_point,
            "interface_scale_percent": self.config.scale,
            "language": if self.config.language == Language::En { "en" } else { "pt-BR" },
            "workspace": (!self.config.welcome_empty).then_some(match self.config.workspace {
                Workspace::Design => "design",
                Workspace::Stock => "stock",
                Workspace::CutPlan => "cut-plan",
                Workspace::Hardware => "hardware",
                Workspace::Handoff => "handoff",
            }),
            "settings_section": self.config.settings.map(|section| format!("{section:?}")),
            "handoff_page": self.config.page,
            "camera": "reference-orthographic",
            "frame": self.frames,
            "capture_snap": self.config.snap.map(|mode| mode.as_str()),
            "capture_dialog": self.config.dialog.map(Dialog::name),
            "capture_handoff_review_state": (self.config.workspace == Workspace::Handoff).then_some("fixture packet pre-reviewed for static capture; not user approval"),
        });
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.config.directory.join("manifest.json"))?;
        serde_json::to_writer_pretty(&mut file, &manifest)?;
        file.write_all(b"\n")?;
        file.sync_all()
    }
}

fn screenshot_request_frame(frame: u32) -> bool {
    frame >= 12 && (frame - 12).is_multiple_of(60)
}

fn write_ppm(path: &Path, image: &egui::ColorImage) -> io::Result<()> {
    if image.size[0].checked_mul(image.size[1]) != Some(image.pixels.len()) {
        return Err(io::Error::other("Invalid native screenshot dimensions"));
    }
    let mut file = io::BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
    write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1])?;
    for color in &image.pixels {
        let [r, g, b, _] = color.to_srgba_unmultiplied();
        file.write_all(&[r, g, b])?;
    }
    file.flush()?;
    file.get_ref().sync_all()
}

#[cfg(test)]
mod tests;
