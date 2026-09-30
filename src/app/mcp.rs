//! `--mcp`: a headless Model Context Protocol server over stdio. An agent
//! builds furniture with the same validated edits as the app and saves a
//! `.pmcab` file; no window opens. stdout carries the protocol, so nothing on
//! this path may print to it: diagnostics go to stderr.
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::service::{Workspace, WorkspaceConfig};

mod server;

pub const HELP: &str = "       plan-my-cabinet --mcp [--open FILE.pmcab] [--catalog-dir DIR] [--user-data-dir DIR] [--language en|pt-BR]\n\
    Runs a Model Context Protocol server on stdin/stdout for AI agents (no window).";

#[derive(Debug, Default)]
struct Options {
    open: Option<PathBuf>,
    catalog_dir: Option<PathBuf>,
    user_data_dir: Option<PathBuf>,
    language: Language,
}

impl Options {
    fn parse(args: &[OsString]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let mut value = |name: &str| {
                args.next()
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("{name} needs a value"))
            };
            match arg.to_str() {
                Some("--open") => options.open = Some(value("--open")?),
                Some("--catalog-dir") => options.catalog_dir = Some(value("--catalog-dir")?),
                Some("--user-data-dir") => options.user_data_dir = Some(value("--user-data-dir")?),
                Some("--language") => {
                    options.language = match value("--language")?.to_str() {
                        Some("en") => Language::En,
                        Some("pt-BR") | Some("pt") => Language::PtBr,
                        _ => return Err("--language is en or pt-BR".into()),
                    }
                }
                _ => return Err(format!("Unknown --mcp option: {}", arg.to_string_lossy())),
            }
        }
        Ok(options)
    }
}

pub(crate) fn run(args: &[OsString]) -> ExitCode {
    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n{HELP}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("plan-my-cabinet mcp: cannot start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut workspace = Workspace::new(WorkspaceConfig {
        catalog_dir: options.catalog_dir,
        user_data_dir: options.user_data_dir,
        language: options.language,
    });
    if let Some(path) = options.open {
        let open = plan_my_cabinet::service::project::OpenProjectInput {
            path: path.display().to_string(),
            discard_changes: false,
        };
        if let Err(error) = workspace.open_project(open) {
            eprintln!(
                "plan-my-cabinet mcp: cannot open {}: {}",
                path.display(),
                error.message
            );
            return ExitCode::FAILURE;
        }
    }
    match runtime.block_on(server::serve(workspace)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("plan-my-cabinet mcp: {error}");
            ExitCode::FAILURE
        }
    }
}
