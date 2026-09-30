//! Project and session tools: new/open/save/close, summary, settings, undo.
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::allocation_diagnostics::{Status, diagnose};
use crate::commands::ProjectEditor;
use crate::cost_estimate::{Feasibility, estimate};
use crate::domain::Project;
use crate::i18n::Language;
use crate::material_presets::seed_defaults;
use crate::money::Currency;
use crate::persistence::{self, EXTENSION};
use crate::service::dto::{
    Change, CurrencyCode, LengthInput, LengthOut, MoneyInput, MoneyOut, UnitName, currency_code,
    money_out, unit_name,
};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::service::workspace::{Document, Kind, Workspace};
use crate::stock_commands::ProjectCurrencyChange;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum LanguageName {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "pt-BR", alias = "pt")]
    PtBr,
}

impl From<LanguageName> for Language {
    fn from(language: LanguageName) -> Self {
        match language {
            LanguageName::En => Language::En,
            LanguageName::PtBr => Language::PtBr,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct NewProjectInput {
    /// Project name shown in the app.
    pub name: String,
    /// Currency for prices and the cut fee. Default BRL.
    #[serde(default)]
    pub currency: Option<CurrencyCode>,
    /// Unit the app displays lengths in. Tool inputs are always millimetres
    /// unless a string carries a unit. Default mm.
    #[serde(default)]
    pub display_unit: Option<UnitName>,
    /// Add the standard sheet materials (white/raw MDF 6–25 mm, HDF 3 mm, …)
    /// that have known sheet sizes, so add_needed_sheets works. Default true.
    #[serde(default = "default_true")]
    pub seed_standard_materials: bool,
    /// Language of the seeded material names. Default en.
    #[serde(default)]
    pub language: Option<LanguageName>,
    /// Replace an open project that has unsaved changes.
    #[serde(default)]
    pub discard_changes: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct OpenProjectInput {
    /// Absolute path of a .pmcab file.
    pub path: String,
    #[serde(default)]
    pub discard_changes: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SaveProjectInput {
    /// Absolute .pmcab path. Required the first time; omit to save in place.
    #[serde(default)]
    pub path: Option<String>,
    /// Replace an existing file at a new path. Ask the user first.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CloseProjectInput {
    #[serde(default)]
    pub discard_changes: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ProjectSettingsInput {
    /// New project name.
    #[serde(default)]
    pub name: Option<String>,
    /// Unit the app displays (not an undo step).
    #[serde(default)]
    pub display_unit: Option<UnitName>,
    /// Snapping grid used by the app's 3D view.
    #[serde(default)]
    pub grid_spacing: Option<LengthInput>,
    /// Saw blade width included in every cut (default 5 mm). Changing it
    /// clears the shop confirmation.
    #[serde(default)]
    pub cutting_kerf: Option<LengthInput>,
    /// Record that the shop confirmed the current kerf (needed for a
    /// shop-ready handoff).
    #[serde(default)]
    pub confirm_shop_kerf: bool,
    /// Price per cut in the project currency; null means unknown.
    #[serde(default, deserialize_with = "crate::service::dto::double_option")]
    pub cut_fee: Option<Option<MoneyInput>>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CurrencyMode {
    /// Keep the numbers, change the currency label.
    Relabel,
    /// Give new prices for every stock piece (and the cut fee).
    Replace,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct StockPriceInput {
    pub stock: String,
    pub price: Option<MoneyInput>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ChangeCurrencyInput {
    pub currency: CurrencyCode,
    pub mode: CurrencyMode,
    /// For replace: the cut fee in the new currency (null = unknown).
    #[serde(default)]
    pub cut_fee: Option<MoneyInput>,
    /// For replace: every stock piece with its new price (null = unknown).
    #[serde(default)]
    pub stock_prices: Vec<StockPriceInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct StepsInput {
    /// How many steps (default 1).
    #[serde(default)]
    pub steps: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectSummary {
    pub open: bool,
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub revision: u64,
    pub saved_revision: Option<u64>,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub currency: &'static str,
    pub display_unit: &'static str,
    pub grid_spacing: LengthOut,
    pub cutting_kerf: LengthOut,
    pub shop_kerf_confirmed: bool,
    pub cut_fee: Option<MoneyOut>,
    pub counts: Value,
    pub allocation: Value,
    pub estimate: Value,
}

pub(crate) fn summary(document: &Document) -> ProjectSummary {
    let editor = &document.editor;
    let project = editor.project();
    let unit = project.display_unit;
    let diagnostics = diagnose(project);
    let count = |status: Status| diagnostics.iter().filter(|d| d.status == status).count();
    let estimate = match estimate(project) {
        Ok(e) => json!({
            "feasibility": match e.feasibility { Feasibility::Verified => "verified", Feasibility::Incomplete => "incomplete" },
            "material": money_out(e.material),
            "cutting": money_out(e.cutting),
            "total": money_out(e.total),
            "sheets_used": e.used_stock.len(),
        }),
        Err(e) => json!({ "error": format!("{e:?}") }),
    };
    ProjectSummary {
        open: true,
        id: project.id.to_string(),
        name: project.name.clone(),
        path: document.path.as_ref().map(|p| p.display().to_string()),
        revision: project.revision,
        saved_revision: editor.saved_revision(),
        dirty: editor.is_dirty(),
        can_undo: editor.can_undo(),
        can_redo: editor.can_redo(),
        currency: currency_code(project.currency),
        display_unit: unit_name(unit),
        grid_spacing: LengthOut::new(project.grid_spacing, unit),
        cutting_kerf: LengthOut::new(project.cutting_kerf, unit),
        shop_kerf_confirmed: project.confirmed_shop_kerf == Some(project.cutting_kerf),
        cut_fee: money_out(project.cut_fee),
        counts: json!({
            "materials": project.materials.len(),
            "boards": project.boards.len(),
            "assemblies": project.assemblies.len(),
            "hardware": project.hardware.len(),
            "stock": project.stock.len(),
            "allocations": project.allocations.len(),
            "catalog": project.catalog.len(),
            "hinges": project.hinge_installations.len(),
            "doors": project.door_joints.len(),
        }),
        allocation: json!({
            "valid": count(Status::AllocatedValid),
            "unallocated": count(Status::Unallocated),
            "conflicted": count(Status::Conflicted),
            "unknown": count(Status::UnknownSearchBudget),
        }),
        estimate,
    }
}

fn pmcab_path(text: &str) -> ServiceResult<PathBuf> {
    let path = PathBuf::from(text);
    if !path.is_absolute() {
        return Err(ServiceError::invalid(format!(
            "'{text}' is not an absolute path"
        )));
    }
    if path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
        return Err(ServiceError::invalid(format!(
            "the file must end in .{EXTENSION}"
        )));
    }
    Ok(path)
}

impl Workspace {
    fn guard_unsaved(&self, discard: bool) -> ServiceResult<()> {
        if let Some(document) = &self.document
            && document.editor.is_dirty()
            && !discard
        {
            return Err(ServiceError::new(
                ErrorCode::UnsavedChanges,
                format!("'{}' has unsaved changes", document.editor.project().name),
            )
            .hint("Save it with save_project, or pass discard_changes: true."));
        }
        Ok(())
    }

    fn replace_document(&mut self, document: Document) {
        self.document = Some(document);
        self.searches.clear();
    }

    fn remember(&self, path: &Path, project: &Project) {
        let Some(dir) = &self.user_data_dir else {
            return;
        };
        match crate::recent_projects::RecentProjects::open(dir) {
            Ok(mut recents) => {
                if let Err(e) =
                    recents.register_successful(path, project.id, std::time::SystemTime::now())
                {
                    eprintln!("plan-my-cabinet mcp: could not record a recent project: {e}");
                }
            }
            Err(e) => eprintln!("plan-my-cabinet mcp: could not open recent projects: {e}"),
        }
    }

    pub fn new_project(&mut self, input: NewProjectInput) -> ServiceResult<ProjectSummary> {
        self.guard_unsaved(input.discard_changes)?;
        let name = crate::board_commands::project_name(&input.name)?.to_owned();
        let currency: Currency = input.currency.unwrap_or(CurrencyCode::BRL).into();
        let mut project = Project::new(&name, currency);
        if let Some(unit) = input.display_unit {
            project.display_unit = unit.into();
        }
        if input.seed_standard_materials {
            let language = input.language.map_or(self.language, Language::from);
            seed_defaults(&mut project, language);
        }
        // A fresh project starts clean; it is not on disk until save_project.
        let editor = ProjectEditor::new(project)?;
        self.replace_document(Document { editor, path: None });
        Ok(summary(self.document()?))
    }

    pub fn open_project(&mut self, input: OpenProjectInput) -> ServiceResult<ProjectSummary> {
        self.guard_unsaved(input.discard_changes)?;
        let path = PathBuf::from(&input.path);
        let file = std::fs::File::open(&path).map_err(|e| {
            ServiceError::new(
                ErrorCode::Io,
                format!("cannot open {}: {e}", path.display()),
            )
        })?;
        let prepared = persistence::prepare_reader(file)?;
        let editor = prepared.into_editor();
        self.remember(&path, editor.project());
        self.replace_document(Document {
            editor,
            path: Some(path),
        });
        Ok(summary(self.document()?))
    }

    pub fn save_project(&mut self, input: SaveProjectInput) -> ServiceResult<Value> {
        let document = self
            .document
            .as_mut()
            .ok_or_else(|| ServiceError::new(ErrorCode::NoOpenProject, "no project is open"))?;
        let target = match (&input.path, &document.path) {
            (Some(text), _) => pmcab_path(text)?,
            (None, Some(path)) => path.clone(),
            (None, None) => {
                return Err(ServiceError::invalid(
                    "this project has never been saved; give a path",
                )
                .hint("Pass an absolute path ending in .pmcab."));
            }
        };
        let same = document.path.as_ref() == Some(&target);
        if !same && target.exists() && !input.overwrite {
            return Err(ServiceError::new(
                ErrorCode::Io,
                format!("{} already exists", target.display()),
            )
            .hint("Ask the user, then pass overwrite: true to replace it."));
        }
        if !same && !target.exists() {
            persistence::save_new(&mut document.editor, &target)?;
        } else {
            persistence::save(&mut document.editor, &target)?;
        }
        document.path = Some(target.clone());
        let project = document.editor.project().clone();
        self.remember(&target, &project);
        Ok(json!({
            "saved": true,
            "path": target.display().to_string(),
            "revision": project.revision,
            "dirty": false,
        }))
    }

    pub fn close_project(&mut self, input: CloseProjectInput) -> ServiceResult<Value> {
        self.guard_unsaved(input.discard_changes)?;
        self.document = None;
        self.searches.clear();
        Ok(json!({ "closed": true }))
    }

    pub fn get_project(&self) -> ServiceResult<ProjectSummary> {
        Ok(summary(self.document()?))
    }

    pub fn set_project_settings(
        &mut self,
        input: ProjectSettingsInput,
    ) -> ServiceResult<Change<ProjectSummary>> {
        let rounding = input.allow_rounding;
        if let Some(unit) = input.display_unit {
            self.editor_mut()?.set_display_unit(unit.into());
        }
        let grid = input
            .grid_spacing
            .as_ref()
            .map(|l| l.positive("grid_spacing", rounding))
            .transpose()?;
        let kerf = input
            .cutting_kerf
            .as_ref()
            .map(|l| l.positive("cutting_kerf", rounding))
            .transpose()?;
        let currency = self.project()?.currency;
        let fee = match &input.cut_fee {
            None => None,
            Some(None) => Some(None),
            Some(Some(m)) => Some(Some(m.resolve(currency)?)),
        };
        let mut changes = Vec::new();
        let change = self.change(input.expected_revision, |editor| {
            if let Some(name) = &input.name {
                editor.rename_project(name)?;
                changes.push("name");
            }
            if let Some(grid) = grid {
                editor.set_grid_spacing(grid)?;
                changes.push("grid spacing");
            }
            if let Some(kerf) = kerf {
                editor.set_cutting_kerf(kerf)?;
                changes.push("cutting kerf");
            }
            if let Some(fee) = fee {
                editor.set_cut_fee(fee)?;
                changes.push("cut fee");
            }
            if input.confirm_shop_kerf {
                editor.confirm_shop_kerf()?;
                changes.push("shop kerf confirmation");
            }
            Ok(((), String::new()))
        })?;
        let summary = summary(self.document()?);
        Ok(Change {
            summary: if changes.is_empty() {
                "No settings changed.".into()
            } else {
                format!("Updated {}.", changes.join(", "))
            },
            result: summary,
            committed: change.committed,
            changed: change.changed || input.display_unit.is_some(),
            revision: change.revision,
            dirty: change.dirty,
            undo_steps: change.undo_steps,
            warnings: change.warnings,
        })
    }

    pub fn change_currency(&mut self, input: ChangeCurrencyInput) -> ServiceResult<Change<Value>> {
        let currency: Currency = input.currency.into();
        let change = match input.mode {
            CurrencyMode::Relabel => ProjectCurrencyChange::ConfirmRelabelWithoutConversion,
            CurrencyMode::Replace => {
                let cut_fee = input
                    .cut_fee
                    .as_ref()
                    .map(|m| m.resolve(currency))
                    .transpose()?;
                let mut stock_prices = Vec::new();
                for entry in &input.stock_prices {
                    let id = self.resolve(Kind::Stock, &entry.stock)?;
                    let price = entry
                        .price
                        .as_ref()
                        .map(|m| m.resolve(currency))
                        .transpose()?;
                    stock_prices.push((id, price));
                }
                ProjectCurrencyChange::Replace {
                    cut_fee,
                    stock_prices,
                }
            }
        };
        self.change(input.expected_revision, |editor| {
            editor.change_project_currency(currency, change)?;
            Ok((
                json!({ "currency": currency_code(currency) }),
                format!("Currency is now {}.", currency_code(currency)),
            ))
        })
    }

    pub fn undo(&mut self, input: StepsInput) -> ServiceResult<Value> {
        self.history(input.steps.unwrap_or(1), true)
    }

    pub fn redo(&mut self, input: StepsInput) -> ServiceResult<Value> {
        self.history(input.steps.unwrap_or(1), false)
    }

    fn history(&mut self, steps: usize, undo: bool) -> ServiceResult<Value> {
        let editor = self.editor_mut()?;
        let mut done = 0;
        for _ in 0..steps.max(1) {
            let moved = if undo { editor.undo()? } else { editor.redo()? };
            if !moved {
                break;
            }
            done += 1;
        }
        Ok(json!({
            "steps_done": done,
            "revision": editor.project().revision,
            "dirty": editor.is_dirty(),
            "can_undo": editor.can_undo(),
            "can_redo": editor.can_redo(),
        }))
    }
}
