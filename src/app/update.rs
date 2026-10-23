//! Startup update check and in-place installer.
//!
//! Release builds installed from a package check GitHub once per run. A newer
//! release is offered in a modal; accepting downloads the matching archive,
//! verifies SHA256SUMS, replaces the installed build and launches it. Debug
//! and source-tree builds never check, and an offline check is silent.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;
use fluent_bundle::FluentArgs;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use plan_my_cabinet::i18n::Localizer;

use crate::modal_chrome::{ModalAction, ModalActions, ModalChrome};

const API_LATEST: &str = "https://api.github.com/repos/killertux/plan-my-cabinet/releases/latest";
const USER_AGENT: &str = concat!("plan-my-cabinet/", env!("CARGO_PKG_VERSION"));
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub(crate) version: Version,
    pub(crate) notes_url: String,
    pub(crate) asset: String,
    pub(crate) asset_url: String,
    pub(crate) checksums_url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Platform {
    os: &'static str,
    arch: &'static str,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

fn host_platform() -> Option<Platform> {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        _ => return None,
    };
    Some(Platform { os, arch })
}

fn asset_name(version: &Version, platform: Platform) -> String {
    let extension = if platform.os == "windows" {
        "zip"
    } else {
        "tar.gz"
    };
    format!(
        "plan-my-cabinet-{version}-{}-{}.{extension}",
        platform.os, platform.arch
    )
}

fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("package version is semver")
}

fn release_from_json(
    body: &str,
    current: &Version,
    platform: Platform,
) -> Result<Option<Release>, String> {
    let github: GitHubRelease =
        serde_json::from_str(body).map_err(|error| format!("invalid release data: {error}"))?;
    let tag = github.tag_name.trim();
    let version = Version::parse(tag.trim_start_matches('v'))
        .map_err(|error| format!("invalid release version: {error}"))?;
    if version <= *current {
        return Ok(None);
    }
    let asset = asset_name(&version, platform);
    let asset_url = github
        .assets
        .iter()
        .find(|item| item.name == asset)
        .map(|item| item.browser_download_url.clone())
        .ok_or_else(|| format!("release has no package for this platform ({asset})"))?;
    let checksums_url = github
        .assets
        .iter()
        .find(|item| item.name == "SHA256SUMS")
        .map(|item| item.browser_download_url.clone())
        .ok_or_else(|| "release has no SHA256SUMS".to_owned())?;
    if !asset_url.starts_with("https://") || !checksums_url.starts_with("https://") {
        return Err("release asset URLs must use HTTPS".into());
    }
    if !github.html_url.starts_with("https://") {
        return Err("release page URL must use HTTPS".into());
    }
    Ok(Some(Release {
        version,
        notes_url: github.html_url,
        asset,
        asset_url,
        checksums_url,
    }))
}

fn check_latest() -> Result<Option<Release>, String> {
    let platform = host_platform().ok_or("updates are not available for this platform")?;
    let agent = agent(CHECK_TIMEOUT);
    let mut response = agent
        .get(API_LATEST)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("update check failed: {error}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("cannot read update check: {error}"))?;
    release_from_json(&body, &current_version(), platform)
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(timeout))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .build()
        .into()
}

#[derive(Clone, Debug)]
enum InstallLayout {
    MacApp { bundle: PathBuf },
    UnixBinary { exe: PathBuf },
    WindowsBinary { exe: PathBuf },
}

fn mac_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    if macos.file_name()? != "MacOS" {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

fn installed_layout() -> Option<InstallLayout> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    if exe
        .components()
        .any(|component| component.as_os_str() == "target")
    {
        return None;
    }
    if cfg!(target_os = "macos") {
        return mac_bundle(&exe).map(|bundle| InstallLayout::MacApp { bundle });
    }
    if cfg!(target_os = "windows") {
        return Some(InstallLayout::WindowsBinary { exe });
    }
    if cfg!(target_os = "linux") {
        return Some(InstallLayout::UnixBinary { exe });
    }
    None
}

fn should_check() -> bool {
    if cfg!(debug_assertions) || std::env::var_os("PMCAB_DISABLE_UPDATE_CHECK").is_some() {
        return false;
    }
    installed_layout().is_some()
}

fn unique_sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.{suffix}-{}", std::process::id()))
}

fn cleanup_stale(layout: &InstallLayout) {
    let (directory, prefix, directory_entry) = match layout {
        InstallLayout::MacApp { bundle } => (
            bundle.parent().map(Path::to_path_buf),
            bundle
                .file_name()
                .map(|name| format!("{}.old-", name.to_string_lossy()))
                .unwrap_or_default(),
            true,
        ),
        InstallLayout::UnixBinary { exe } | InstallLayout::WindowsBinary { exe } => (
            exe.parent().map(Path::to_path_buf),
            exe.file_name()
                .map(|name| format!("{}.old-", name.to_string_lossy()))
                .unwrap_or_default(),
            false,
        ),
    };
    let Some(directory) = directory else {
        return;
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(&prefix) {
            continue;
        }
        // The immediately previous update may still be shutting down. Only
        // remove a backup after it has had time to exit.
        let old_enough = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(3600));
        if !old_enough {
            continue;
        }
        if directory_entry {
            let _ = fs::remove_dir_all(entry.path());
        } else {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Downloading,
    Verifying,
    Installing,
}

#[derive(Debug)]
enum Event {
    Downloading { received: u64, total: Option<u64> },
    Verifying,
    Installing,
    Ready,
    Failed(String),
    Cancelled,
}

enum JobError {
    Cancelled,
    Failed(String),
}

impl From<String> for JobError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for JobError {
    fn from(message: &str) -> Self {
        Self::Failed(message.to_owned())
    }
}

struct Job {
    events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    cancel_requested: bool,
    phase: Phase,
    received: u64,
    total: Option<u64>,
}

struct ProgressView {
    phase: Phase,
    cancel_requested: bool,
    received: u64,
    total: Option<u64>,
    cancel: Arc<AtomicBool>,
}

pub(crate) struct UpdateHost {
    check: Option<Receiver<Result<Option<Release>, String>>>,
    available: Option<Release>,
    job: Option<Job>,
    dismissed: bool,
    started: bool,
    error: Option<String>,
    chrome: ModalChrome,
}

impl Default for UpdateHost {
    fn default() -> Self {
        Self {
            check: None,
            available: None,
            job: None,
            dismissed: false,
            started: false,
            error: None,
            chrome: ModalChrome::new(egui::Id::new("update-dialog"))
                .width(520.0)
                .icon(crate::icons::Icon::Globe),
        }
    }
}

impl UpdateHost {
    pub(crate) fn start_check(&mut self, ctx: egui::Context) {
        if self.started {
            return;
        }
        self.started = true;
        if !should_check() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.check = Some(receiver);
        std::thread::spawn(move || {
            let _ = sender.send(check_latest());
            ctx.request_repaint();
        });
    }

    fn start_update(&mut self, ctx: &egui::Context) {
        let Some(release) = self.available.clone() else {
            return;
        };
        if self.job.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.error = None;
        self.job = Some(Job {
            events: receiver,
            cancel: cancel.clone(),
            cancel_requested: false,
            phase: Phase::Downloading,
            received: 0,
            total: None,
        });
        let worker_ctx = ctx.clone();
        std::thread::spawn(move || run_update(release, cancel, sender, worker_ctx));
    }

    pub(crate) fn blocks_ui(&self) -> bool {
        self.chrome.is_active()
    }

    pub(crate) fn has_job(&self) -> bool {
        self.job.is_some()
    }

    #[cfg(test)]
    pub(crate) fn offer_for_test(&mut self, release: Release) {
        self.available = Some(release);
        self.dismissed = false;
        self.error = None;
    }

    #[cfg(test)]
    pub(crate) fn fail_for_test(&mut self, release: Release, error: &str) {
        self.available = Some(release);
        self.error = Some(error.into());
        self.dismissed = false;
    }

    #[cfg(test)]
    pub(crate) fn progress_for_test(&mut self, cancel: Arc<AtomicBool>) {
        let (sender, receiver) = mpsc::channel();
        std::mem::forget(sender);
        self.job = Some(Job {
            events: receiver,
            cancel,
            cancel_requested: false,
            phase: Phase::Downloading,
            received: 50,
            total: Some(100),
        });
    }

    #[cfg(test)]
    pub(crate) fn available_for_test(&self) -> Option<&Release> {
        self.available.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn dismissed_for_test(&self) -> bool {
        self.dismissed
    }

    #[cfg(test)]
    pub(crate) fn cancel_requested_for_test(&self) -> bool {
        self.job.as_ref().is_some_and(|job| job.cancel_requested)
    }

    fn dismiss(&mut self) {
        self.available = None;
        self.error = None;
        self.dismissed = true;
    }

    fn poll(&mut self, ctx: &egui::Context) -> bool {
        let check = self.check.as_ref().map(Receiver::try_recv);
        if let Some(Ok(result)) = check {
            self.check = None;
            if let Ok(Some(release)) = result
                && !self.dismissed
            {
                self.available = Some(release);
            }
        } else if matches!(check, Some(Err(TryRecvError::Disconnected))) {
            self.check = None;
        }

        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(job) = &self.job {
            loop {
                match job.events.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut ready = false;
        for event in events {
            match event {
                Event::Downloading { received, total } => {
                    if let Some(job) = &mut self.job {
                        job.phase = Phase::Downloading;
                        job.received = received;
                        job.total = total;
                    }
                }
                Event::Verifying => {
                    if let Some(job) = &mut self.job {
                        job.phase = Phase::Verifying;
                    }
                }
                Event::Installing => {
                    if let Some(job) = &mut self.job {
                        job.phase = Phase::Installing;
                    }
                }
                Event::Ready => {
                    self.job = None;
                    ready = true;
                }
                Event::Failed(error) => {
                    self.job = None;
                    self.error = Some(error);
                }
                Event::Cancelled => {
                    self.job = None;
                }
            }
        }
        if disconnected && self.job.is_some() {
            self.job = None;
            self.error = Some("the update worker stopped unexpectedly".into());
        }
        if self.job.is_some() {
            ctx.request_repaint_after(PROGRESS_INTERVAL);
        }
        ready
    }

    fn show(&mut self, ctx: &egui::Context, localizer: &Localizer) {
        if let Some(job) = self.job.as_ref() {
            let view = ProgressView {
                phase: job.phase,
                cancel_requested: job.cancel_requested,
                received: job.received,
                total: job.total,
                cancel: job.cancel.clone(),
            };
            self.show_progress(ctx, localizer, view);
        } else if let Some(release) = self.available.clone() {
            self.show_prompt(ctx, localizer, &release);
        } else if self.chrome.is_active() {
            self.chrome.close(ctx);
        }
    }

    fn show_prompt(&mut self, ctx: &egui::Context, localizer: &Localizer, release: &Release) {
        let title = localizer.text("update-title");
        let later = localizer.text("update-later");
        let confirm = localizer.text(if self.error.is_some() {
            "update-retry"
        } else {
            "update-now"
        });
        let mut args = FluentArgs::new();
        args.set("current", current_version().to_string());
        args.set("available", release.version.to_string());
        self.chrome
            .set_context(Some(localizer.format("update-context", Some(&args))));
        let error = self.error.clone();
        let result = self.chrome.show(
            ctx,
            &title,
            ModalActions {
                cancel: &later,
                confirm: &confirm,
            },
            |ui| {
                ui.label(
                    egui::RichText::new(localizer.text("update-description"))
                        .size(12.0)
                        .color(crate::theme_widgets::SECONDARY),
                );
                ui.add_space(6.0);
                ui.hyperlink_to(
                    localizer.text("update-release-notes"),
                    release.notes_url.clone(),
                );
                if let Some(error) = &error {
                    ui.add_space(8.0);
                    let mut args = FluentArgs::new();
                    args.set("error", error.clone());
                    ui.label(
                        egui::RichText::new(localizer.format("update-failed", Some(&args)))
                            .size(12.0)
                            .color(crate::theme_widgets::DANGER),
                    );
                }
                ((), true)
            },
        );
        match result.action {
            ModalAction::Confirm => self.start_update(ctx),
            ModalAction::Cancel => {
                self.dismiss();
                self.chrome.close(ctx);
            }
            ModalAction::None => {}
        }
    }

    fn show_progress(&mut self, ctx: &egui::Context, localizer: &Localizer, view: ProgressView) {
        let title = localizer.text("update-progress-title");
        let status = localizer.text(if view.cancel_requested {
            "update-cancelling"
        } else {
            match view.phase {
                Phase::Downloading => "update-downloading",
                Phase::Verifying => "update-verifying",
                Phase::Installing => "update-installing",
            }
        });
        let back = if view.phase == Phase::Installing {
            localizer.text("update-wait")
        } else if view.cancel_requested {
            localizer.text("update-cancelling")
        } else {
            localizer.text("cancel")
        };
        let result = self.chrome.show_reader(ctx, &title, &back, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new(&status).size(12.0));
            });
            if view.phase == Phase::Downloading
                && let Some(total) = view.total
                && total > 0
            {
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new((view.received as f32 / total as f32).clamp(0.0, 1.0))
                        .show_percentage(),
                );
            }
        });
        if result.action == ModalAction::Cancel
            && view.phase != Phase::Installing
            && !view.cancel_requested
        {
            view.cancel.store(true, Ordering::Relaxed);
            if let Some(job) = &mut self.job {
                job.cancel_requested = true;
            }
        }
    }
}

fn temp_update_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "plan-my-cabinet-update-{}-{stamp}",
        std::process::id()
    ))
}

fn report(sender: &Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = sender.send(event);
    ctx.request_repaint();
}

fn run_update(
    release: Release,
    cancel: Arc<AtomicBool>,
    sender: Sender<Event>,
    ctx: egui::Context,
) {
    let directory = temp_update_dir();
    let result = (|| -> Result<(), JobError> {
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "cannot create update directory {}: {error}",
                directory.display()
            )
        })?;
        let archive = download(&release, &directory, &cancel, &sender, &ctx)?;
        report(&sender, &ctx, Event::Verifying);
        verify(&release, &archive, &cancel)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(JobError::Cancelled);
        }
        report(&sender, &ctx, Event::Installing);
        install(&release, &directory)
    })();
    let _ = fs::remove_dir_all(&directory);
    match result {
        Ok(()) => report(&sender, &ctx, Event::Ready),
        Err(JobError::Cancelled) => report(&sender, &ctx, Event::Cancelled),
        Err(JobError::Failed(error)) => report(&sender, &ctx, Event::Failed(error)),
    }
}

fn download(
    release: &Release,
    directory: &Path,
    cancel: &AtomicBool,
    sender: &Sender<Event>,
    ctx: &egui::Context,
) -> Result<PathBuf, JobError> {
    let mut response = agent(DOWNLOAD_TIMEOUT)
        .get(&release.asset_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("download failed: {error}"))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let path = directory.join(&release.asset);
    let mut file = File::create(&path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let mut reader = response.body_mut().as_reader();
    let mut buffer = [0_u8; 64 * 1024];
    let mut received = 0_u64;
    let mut last = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(JobError::Cancelled);
        }
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("download interrupted: {error}"))?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(|error| format!("cannot write download: {error}"))?;
        received += count as u64;
        if last.elapsed() >= PROGRESS_INTERVAL {
            report(sender, ctx, Event::Downloading { received, total });
            last = Instant::now();
        }
    }
    report(sender, ctx, Event::Downloading { received, total });
    Ok(path)
}

fn verify(release: &Release, archive: &Path, cancel: &AtomicBool) -> Result<(), JobError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(JobError::Cancelled);
    }
    let mut response = agent(CHECK_TIMEOUT)
        .get(&release.checksums_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("cannot download SHA256SUMS: {error}"))?;
    let checksums = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("cannot read SHA256SUMS: {error}"))?;
    let expected = checksum_for(&checksums, &release.asset)
        .ok_or_else(|| format!("SHA256SUMS has no entry for {}", release.asset))?;
    let actual = sha256_file(archive)?;
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(format!(
            "checksum mismatch for {} (expected {expected}, got {actual})",
            release.asset
        )
        .into());
    }
    Ok(())
}

fn checksum_for(checksums: &str, asset: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == asset && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract(release: &Release, directory: &Path) -> Result<PathBuf, JobError> {
    let archive = directory.join(&release.asset);
    let mut command = if cfg!(target_os = "windows") {
        let script = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            powershell_quote(&archive),
            powershell_quote(directory)
        );
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script.as_str(),
        ]);
        command
    } else {
        let mut command = Command::new("tar");
        command.arg("-xzf").arg(&archive).arg("-C").arg(directory);
        command
    };
    #[cfg(windows)]
    no_window(&mut command);
    let status = command
        .status()
        .map_err(|error| format!("cannot extract update: {error}"))?;
    if !status.success() {
        return Err("cannot extract update: the archive tool failed".into());
    }
    let folder = if release.asset.contains("macos") {
        "Plan My Cabinet.app".to_owned()
    } else if release.asset.contains("windows") {
        format!("plan-my-cabinet-{}-windows-x86_64", release.version)
    } else {
        format!("plan-my-cabinet-{}-linux-x86_64", release.version)
    };
    let extracted = directory.join(&folder);
    if !extracted.exists() {
        return Err(format!("update package did not contain {folder}").into());
    }
    Ok(extracted)
}

fn install(release: &Release, directory: &Path) -> Result<(), JobError> {
    let layout = installed_layout().ok_or("this build cannot update itself")?;
    cleanup_stale(&layout);
    let extracted = extract(release, directory)?;
    match layout {
        InstallLayout::MacApp { bundle } => {
            let new_bundle = extracted.join("Plan My Cabinet.app");
            let backup = unique_sibling(&bundle, "old");
            let _ = fs::remove_dir_all(&backup);
            fs::rename(&bundle, &backup)
                .map_err(|error| format!("cannot move the current app: {error}"))?;
            if let Err(error) = fs::rename(&new_bundle, &bundle) {
                let _ = fs::rename(&backup, &bundle);
                return Err(format!("cannot install the new app: {error}").into());
            }
            let launched = Command::new("open").arg("-n").arg(&bundle).status();
            match launched {
                Ok(status) if status.success() => Ok(()),
                Ok(status) => {
                    let _ = fs::rename(&bundle, &new_bundle);
                    let _ = fs::rename(&backup, &bundle);
                    Err(format!("update launch failed with {status}").into())
                }
                Err(error) => {
                    let _ = fs::rename(&bundle, &new_bundle);
                    let _ = fs::rename(&backup, &bundle);
                    Err(format!("cannot launch the new app: {error}").into())
                }
            }
        }
        InstallLayout::UnixBinary { exe } => {
            let new_exe = extracted.join("bin").join("plan-my-cabinet");
            let backup = replace_binary(&exe, &new_exe)?;
            if let Err(error) = spawn_binary(&exe) {
                let _ = fs::remove_file(&exe);
                let _ = fs::rename(&backup, &exe);
                return Err(error);
            }
            Ok(())
        }
        InstallLayout::WindowsBinary { exe } => {
            let new_exe = extracted.join("plan-my-cabinet.exe");
            let backup = replace_binary(&exe, &new_exe)?;
            if let Err(error) = spawn_binary(&exe) {
                let _ = fs::remove_file(&exe);
                let _ = fs::rename(&backup, &exe);
                return Err(error);
            }
            Ok(())
        }
    }
}

fn replace_binary(exe: &Path, new_exe: &Path) -> Result<PathBuf, JobError> {
    if !new_exe.is_file() {
        return Err(format!("update package did not contain {}", new_exe.display()).into());
    }
    let backup = unique_sibling(exe, "old");
    let _ = fs::remove_file(&backup);
    fs::rename(exe, &backup)
        .map_err(|error| format!("cannot move the current executable: {error}"))?;
    if let Err(error) = fs::copy(new_exe, exe) {
        let _ = fs::rename(&backup, exe);
        return Err(format!("cannot install the new executable: {error}").into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = fs::set_permissions(exe, fs::Permissions::from_mode(0o755)) {
            let _ = fs::remove_file(exe);
            let _ = fs::rename(&backup, exe);
            return Err(format!("cannot make the new executable runnable: {error}").into());
        }
    }
    Ok(backup)
}

fn spawn_binary(exe: &Path) -> Result<(), JobError> {
    let mut command = Command::new(exe);
    if let Some(parent) = exe.parent() {
        command.current_dir(parent);
    }
    #[cfg(windows)]
    no_window(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("cannot launch the new version: {error}").into())
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

fn powershell_quote(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

impl crate::DesktopApp {
    pub(crate) fn start_update_check(&mut self, ctx: &egui::Context) {
        self.update.start_check(ctx.clone());
    }

    pub(crate) fn show_update(&mut self, ctx: &egui::Context) {
        if self.update.poll(ctx) {
            self.project_files.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if !self.project_files.welcome.visible {
            // A check that finishes after the user opened a project is
            // discarded; never interrupt or restart active work.
            if !self.update.has_job() {
                self.update.dismiss();
                self.update.show(ctx, &self.localizer);
            }
            return;
        }
        if self.external_modal_open() && !self.update.blocks_ui() {
            return;
        }
        self.update.show(ctx, &self.localizer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORM: Platform = Platform {
        os: "macos",
        arch: "arm64",
    };

    fn release_json(tag: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://example.test/release","assets":[
                {{"name":"plan-my-cabinet-0.9.0-macos-arm64.tar.gz","browser_download_url":"https://example.test/app.tar.gz"}},
                {{"name":"plan-my-cabinet-0.9.0-linux-x86_64.tar.gz","browser_download_url":"https://example.test/linux.tar.gz"}},
                {{"name":"plan-my-cabinet-0.9.0-windows-x86_64.zip","browser_download_url":"https://example.test/win.zip"}},
                {{"name":"SHA256SUMS","browser_download_url":"https://example.test/SHA256SUMS"}}
            ]}}"#
        )
    }

    #[test]
    fn newer_versions_are_offered_and_current_ones_are_not() {
        let current = Version::parse("0.8.0").unwrap();
        assert!(
            release_from_json(&release_json("v0.9.0"), &current, PLATFORM)
                .unwrap()
                .is_some()
        );
        assert!(
            release_from_json(&release_json("v0.8.0"), &current, PLATFORM)
                .unwrap()
                .is_none()
        );
        assert!(
            release_from_json(&release_json("v0.7.0"), &current, PLATFORM)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn the_asset_matches_the_running_platform() {
        let version = Version::parse("0.9.0").unwrap();
        assert_eq!(
            asset_name(&version, PLATFORM),
            "plan-my-cabinet-0.9.0-macos-arm64.tar.gz"
        );
        assert_eq!(
            asset_name(
                &version,
                Platform {
                    os: "windows",
                    arch: "x86_64"
                }
            ),
            "plan-my-cabinet-0.9.0-windows-x86_64.zip"
        );
        let release = release_from_json(
            &release_json("v0.9.0"),
            &Version::parse("0.8.0").unwrap(),
            PLATFORM,
        )
        .unwrap()
        .unwrap();
        assert_eq!(release.asset, "plan-my-cabinet-0.9.0-macos-arm64.tar.gz");
        assert_eq!(release.checksums_url, "https://example.test/SHA256SUMS");
    }

    #[test]
    fn checksum_lines_accept_sha256sum_star_and_plain_forms() {
        let sums = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  plan-my-cabinet-0.9.0-macos-arm64.tar.gz\n\
                    abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789 *plan-my-cabinet-0.8.0-macos-arm64.tar.gz\n";
        assert_eq!(
            checksum_for(sums, "plan-my-cabinet-0.9.0-macos-arm64.tar.gz").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            checksum_for(sums, "plan-my-cabinet-0.8.0-macos-arm64.tar.gz").as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert_eq!(checksum_for(sums, "missing.tar.gz"), None);
    }

    #[test]
    fn mac_bundles_are_recognized_and_plain_executables_are_not() {
        assert_eq!(
            mac_bundle(Path::new(
                "/Applications/Plan My Cabinet.app/Contents/MacOS/plan-my-cabinet"
            )),
            Some(PathBuf::from("/Applications/Plan My Cabinet.app"))
        );
        assert_eq!(
            mac_bundle(Path::new("/usr/local/bin/plan-my-cabinet")),
            None
        );
    }

    #[test]
    fn debug_builds_never_check_or_install() {
        assert!(!should_check());
    }

    #[test]
    fn later_persists_for_this_run_only() {
        let mut host = UpdateHost {
            available: release_from_json(
                &release_json("v0.9.0"),
                &Version::parse("0.8.0").unwrap(),
                PLATFORM,
            )
            .unwrap(),
            ..Default::default()
        };
        host.dismiss();
        assert!(host.available.is_none());
        assert!(host.dismissed);
    }
}
