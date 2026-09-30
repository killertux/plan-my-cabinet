//! Stock, cut plan and optimizer tools.
use std::collections::HashSet;
use std::time::Duration;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::allocation_diagnostics::{Status, diagnose};
use crate::candidate_generation::SearchBudget;
use crate::candidate_ranking::{Objective, RankedCandidate};
use crate::cut_tree::{Axis, Edge};
use crate::domain::Project;
use crate::material_changes::allocation_conflicts;
use crate::optimization_worker::OptimizationWorker;
use crate::read_models::stock_read_models::{SheetProof, StockPieceReadModel, StockReadModel};
use crate::service::design::conflicts_json;
use crate::service::dto::{
    Change, LengthInput, LengthOut, MoneyInput, SheetGrain, Source, grain_name, mm_f64, money_out,
    sheet_grain_name, source_name,
};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult, sheet_diagnostics_json};
use crate::service::workspace::{Kind, StoredSearch, Workspace, object_name};
use crate::sheet_edit::{SheetEditError, SheetEditSession, SheetStatus};
use crate::sheet_packer::{PackMode, Unplaced, suggest_sheets};
use crate::stock_commands::StockInput;
use crate::units::Length;

fn unplaced_name(reason: Unplaced) -> &'static str {
    match reason {
        Unplaced::NoStock => "no_stock: no sheet of this material and thickness is declared",
        Unplaced::TooLarge => "too_large: larger than every sheet of its material",
        Unplaced::Grain => "grain: fits only turned, which the grain forbids",
        Unplaced::NoRoom => "no_room: the declared sheets are full",
    }
}

fn read_model(project: &Project) -> ServiceResult<StockReadModel> {
    StockReadModel::build(project)
        .map_err(|e| ServiceError::new(ErrorCode::InvalidProject, format!("{e:?}")))
}

fn proof_name(proof: &SheetProof) -> &'static str {
    match proof {
        SheetProof::Unused => "unused",
        SheetProof::Verified { .. } => "verified",
        SheetProof::Violation(_) => "violation",
        SheetProof::SearchExhausted => "search_exhausted",
    }
}

fn percent(ratio: Option<(i128, i128)>) -> Option<f64> {
    ratio.and_then(|(part, whole)| {
        (whole > 0).then(|| (part as f64 / whole as f64 * 1000.0).round() / 10.0)
    })
}

fn piece_row(project: &Project, piece: &StockPieceReadModel) -> Value {
    let unit = project.display_unit;
    json!({
        "id": piece.id,
        "alias": piece.alias,
        "rank": piece.global_rank,
        "name": piece.name,
        "material": piece.material_name,
        "length": LengthOut::new(piece.length, unit),
        "width": LengthOut::new(piece.width, unit),
        "thickness": LengthOut::new(piece.measured_thickness, unit),
        "grain": sheet_grain_name(piece.grain),
        "source": source_name(piece.source),
        "price": money_out(piece.price),
        "trim_mm": piece.trim.map(mm_f64),
        "parts": piece.parts.len(),
        "status": proof_name(&piece.proof),
        "cuts": piece.proof.cut_count(),
        "utilization_percent": percent(piece.proof.utilization()),
    })
}

fn cut_sequence(piece: &StockPieceReadModel) -> Value {
    let Some((tree, accounting)) = piece.proof.verified() else {
        return Value::Null;
    };
    let ops: Vec<_> = tree
        .operations()
        .into_iter()
        .map(|op| {
            let input = tree.node(op.input).map(|n| n.rectangle);
            json!({
                "cut": op.number,
                "direction": match op.axis { Axis::X => "across the length (a cut perpendicular to X)", Axis::Y => "along the length (a cut perpendicular to Y)" },
                "piece_origin_mm": input.map(|r| r.origin.map(mm_f64)),
                "piece_size_mm": input.map(|r| r.extent.map(mm_f64)),
                "measure_from": match op.reference_edge { Edge::Low => "low edge", Edge::High => "high edge" },
                "keep_mm": mm_f64(op.retained_extent),
                "kerf_on": match op.kerf_side { Edge::Low => "low side", Edge::High => "high side" },
            })
        })
        .collect();
    let mm2 = |a: i128| (a as f64 / 1.0e6).round();
    json!({
        "cuts": ops,
        "area_mm2": {
            "sheet": mm2(accounting.root_area),
            "parts": mm2(accounting.part_area),
            "offcuts": mm2(accounting.offcut_area),
            "waste": mm2(accounting.waste_area),
            "kerf": mm2(accounting.kerf_loss),
            "trim": mm2(accounting.trim_loss),
        },
    })
}

// ------------------------------------------------------------------- inputs

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct ListStockInput {
    #[serde(default)]
    pub material: Option<String>,
    /// Only pieces with parts on them.
    #[serde(default)]
    pub used_only: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct StockRefInput {
    /// Alias ("S1"), name or id.
    pub stock: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct CreateStockInput {
    /// Default "<material> <length>×<width>".
    #[serde(default)]
    pub name: Option<String>,
    pub material: String,
    /// Sheet length (its X, usually the long side).
    pub length: LengthInput,
    pub width: LengthInput,
    /// Measured thickness; default the material's.
    #[serde(default)]
    pub thickness: Option<LengthInput>,
    /// Default unknown. along_x = grain along the sheet length.
    #[serde(default)]
    pub grain: Option<SheetGrain>,
    /// Default to_purchase.
    #[serde(default)]
    pub source: Option<Source>,
    /// Price per piece in the project currency; omit if unknown.
    #[serde(default)]
    pub price: Option<MoneyInput>,
    /// Unusable edge strips [left, right, bottom, top], including the blade
    /// width of the trimming pass. Default none.
    #[serde(default)]
    pub trim_mm: Option<[LengthInput; 4]>,
    /// Number of identical pieces (default 1).
    #[serde(default)]
    pub quantity: Option<u32>,
    /// Then place waiting boards on the new stock (default true).
    #[serde(default = "yes")]
    pub place_waiting: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct UpdateStockInput {
    pub stock: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub length: Option<LengthInput>,
    #[serde(default)]
    pub width: Option<LengthInput>,
    #[serde(default)]
    pub thickness: Option<LengthInput>,
    #[serde(default)]
    pub grain: Option<SheetGrain>,
    #[serde(default)]
    pub source: Option<Source>,
    /// null clears the price (unknown).
    #[serde(default, deserialize_with = "crate::service::dto::double_option")]
    pub price: Option<Option<MoneyInput>>,
    #[serde(default)]
    pub trim_mm: Option<[LengthInput; 4]>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct PriceEntry {
    /// A stock piece, a material name (all its pieces), or "all".
    pub stock: String,
    /// null = unknown.
    pub price: Option<MoneyInput>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SetPricesInput {
    pub prices: Vec<PriceEntry>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DuplicateStockInput {
    pub stock: String,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DeleteStockInput {
    pub stock: String,
    /// Take its parts off first (they become unallocated).
    #[serde(default)]
    pub unallocate_first: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ReorderStockInput {
    pub stock: String,
    /// New 1-based position in the fill order (1 = used first).
    pub rank: usize,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct AddNeededSheetsInput {
    /// Only this material (default: every material with waiting boards).
    #[serde(default)]
    pub material: Option<String>,
    /// Override the suggested count (with `material`).
    #[serde(default)]
    pub count: Option<usize>,
    /// Price per new sheet (with `material`).
    #[serde(default)]
    pub price: Option<MoneyInput>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackModeName {
    /// Place waiting boards; never move placed ones (default).
    #[default]
    FillGaps,
    /// Repack everything that is not locked, usually onto fewer sheets.
    Replan,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct AutoPlaceInput {
    #[serde(default)]
    pub mode: PackModeName,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct CutPlanInput {
    /// Include every sheet's numbered cut sequence.
    #[serde(default)]
    pub include_cuts: bool,
}

/// One manual cut-plan operation.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SheetOp {
    /// Put (or move) a board on a sheet at a position from the sheet's
    /// bottom-left corner. quarter_turn swaps its length and width.
    Place {
        board: String,
        stock: String,
        x: LengthInput,
        y: LengthInput,
        #[serde(default)]
        quarter_turn: bool,
    },
    /// Take a board off its sheet.
    Unallocate { board: String },
    /// Lock or unlock a board's placement (locked boards survive replan and
    /// optimization).
    Lock { board: String, locked: bool },
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct EditSheetInput {
    /// Applied in order; all or nothing.
    pub ops: Vec<SheetOp>,
    /// Check without committing.
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    #[serde(default)]
    pub allow_rounding: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveName {
    /// Least money for sheets to buy and cuts (default).
    #[default]
    LowestNewSpending,
    FewestCuts,
    LeastUnusedStockArea,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
pub struct OptimizeInput {
    #[serde(default)]
    pub objective: ObjectiveName,
    /// Search time limit, 1–60 s (default 10).
    #[serde(default)]
    pub max_seconds: Option<f64>,
    /// How many candidates to return (default 3).
    #[serde(default)]
    pub top: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ApplyOptimizationInput {
    pub search_id: String,
    /// 0 = best (default).
    #[serde(default)]
    pub candidate: Option<usize>,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

fn place_summary_json(project: &Project, summary: &crate::auto_place::PlaceSummary) -> Value {
    json!({
        "placed_or_moved": summary.placed,
        "sheets_added": summary.sheets_added,
        "unplaced": summary.unplaced.iter().map(|(id, reason)| json!({
            "board_id": id, "board": object_name(project, *id), "reason": unplaced_name(*reason)
        })).collect::<Vec<_>>(),
    })
}

impl Workspace {
    pub fn list_stock(&self, input: ListStockInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let material = input
            .material
            .as_ref()
            .map(|m| self.resolve(Kind::Material, m))
            .transpose()?;
        let model = read_model(project)?;
        let rows: Vec<_> = model
            .pieces
            .iter()
            .filter(|p| material.is_none_or(|m| p.material_id == m))
            .filter(|p| !input.used_only || p.is_used())
            .map(|p| piece_row(project, p))
            .collect();
        Ok(
            json!({ "revision": project.revision, "fill_order": "sheets are filled from rank 1 down", "stock": rows }),
        )
    }

    pub fn get_stock_piece(&self, input: StockRefInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let id = self.resolve(Kind::Stock, &input.stock)?;
        let model = read_model(project)?;
        let piece = model
            .pieces
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| ServiceError::not_found("stock piece", &input.stock))?;
        let mut row = piece_row(project, piece);
        row["parts"] = json!(
            piece
                .parts
                .iter()
                .map(|p| json!({
                    "board_id": p.board_id,
                    "name": p.name,
                    "origin_mm": p.origin.map(mm_f64),
                    "size_mm": [mm_f64(p.length), mm_f64(p.width)],
                    "quarter_turn": p.quarter_turn,
                    "locked": p.locked,
                    "grain": grain_name(p.effective_grain),
                }))
                .collect::<Vec<_>>()
        );
        row["cut_sequence"] = cut_sequence(piece);
        Ok(row)
    }

    pub fn create_stock(&mut self, input: CreateStockInput) -> ServiceResult<Change<Value>> {
        let project = self.project()?;
        let material_id = self.resolve(Kind::Material, &input.material)?;
        let material = project
            .material(material_id)
            .cloned()
            .ok_or_else(|| ServiceError::not_found("material", material_id))?;
        let r = input.allow_rounding;
        let length = input.length.positive("length", r)?;
        let width = input.width.positive("width", r)?;
        let thickness = match &input.thickness {
            Some(t) => t.positive("thickness", r)?,
            None => material.default_thickness,
        };
        let trim = match &input.trim_mm {
            Some(t) => [
                t[0].non_negative("trim left", r)?,
                t[1].non_negative("trim right", r)?,
                t[2].non_negative("trim bottom", r)?,
                t[3].non_negative("trim top", r)?,
            ],
            None => [Length::ZERO; 4],
        };
        let price = input
            .price
            .as_ref()
            .map(|m| m.resolve(project.currency))
            .transpose()?;
        let name = input
            .name
            .clone()
            .unwrap_or_else(|| format!("{} {}×{}", material.name, mm_f64(length), mm_f64(width)));
        let stock_input = StockInput {
            name,
            material_id,
            length,
            width,
            thickness,
            grain: input
                .grain
                .map_or(crate::domain::StockGrain::Unknown, Into::into),
            source: input
                .source
                .map_or(crate::domain::StockSource::ToPurchase, Into::into),
            price,
            trim,
        };
        let quantity = input.quantity.unwrap_or(1);
        let mut change = self.change(input.expected_revision, |editor| {
            let ids = editor.create_stock(stock_input, quantity)?;
            let placed = if input.place_waiting {
                Some(editor.auto_place(PackMode::FillGaps)?)
            } else {
                None
            };
            let project = editor.project();
            let aliases: Vec<_> = ids.iter().map(|id| project.stock_alias(*id)).collect();
            Ok((
                json!({
                    "stock_ids": ids,
                    "aliases": aliases,
                    "placement": placed.as_ref().map(|p| place_summary_json(project, p)),
                }),
                format!("Added {quantity} stock piece(s)."),
            ))
        })?;
        if price.is_none() {
            change.warnings.push(
                "No price given: cost estimates stay incomplete until set_stock_prices.".into(),
            );
        }
        Ok(change)
    }

    pub fn update_stock(&mut self, input: UpdateStockInput) -> ServiceResult<Change<Value>> {
        let project = self.project()?;
        let id = self.resolve(Kind::Stock, &input.stock)?;
        let current = project
            .stock_piece(id)
            .ok_or_else(|| ServiceError::not_found("stock piece", &input.stock))?;
        let mut next = StockInput::from(current);
        let r = input.allow_rounding;
        if let Some(name) = &input.name {
            next.name = name.clone();
        }
        if let Some(material) = &input.material {
            next.material_id = self.resolve(Kind::Material, material)?;
        }
        if let Some(v) = &input.length {
            next.length = v.positive("length", r)?;
        }
        if let Some(v) = &input.width {
            next.width = v.positive("width", r)?;
        }
        if let Some(v) = &input.thickness {
            next.thickness = v.positive("thickness", r)?;
        }
        if let Some(g) = input.grain {
            next.grain = g.into();
        }
        if let Some(s) = input.source {
            next.source = s.into();
        }
        if let Some(price) = &input.price {
            next.price = price
                .as_ref()
                .map(|m| m.resolve(project.currency))
                .transpose()?;
        }
        if let Some(t) = &input.trim_mm {
            next.trim = [
                t[0].non_negative("trim left", r)?,
                t[1].non_negative("trim right", r)?,
                t[2].non_negative("trim bottom", r)?,
                t[3].non_negative("trim top", r)?,
            ];
        }
        self.change(input.expected_revision, |editor| {
            editor.edit_stock(id, next)?;
            let project = editor.project();
            let conflicts: Vec<_> = allocation_conflicts(project)
                .into_iter()
                .filter(|c| c.stock_id == id)
                .collect();
            Ok((
                json!({ "conflicts": conflicts_json(project, &conflicts) }),
                format!("Updated {}.", project.stock_alias(id).unwrap_or_default()),
            ))
        })
    }

    pub fn set_stock_prices(&mut self, input: SetPricesInput) -> ServiceResult<Change<Value>> {
        let project = self.project()?;
        let currency = project.currency;
        let mut updates: Vec<(Uuid, Option<crate::money::Money>)> = Vec::new();
        for entry in &input.prices {
            let price = entry
                .price
                .as_ref()
                .map(|m| m.resolve(currency))
                .transpose()?;
            let targets: Vec<Uuid> = if entry.stock.eq_ignore_ascii_case("all") {
                project.stock.iter().map(|s| s.id).collect()
            } else if let Ok(id) = self.resolve(Kind::Stock, &entry.stock) {
                vec![id]
            } else {
                let material = self.resolve(Kind::Material, &entry.stock).map_err(|_| {
                    ServiceError::not_found("stock piece or material", &entry.stock)
                })?;
                project
                    .stock
                    .iter()
                    .filter(|s| s.material_id == material)
                    .map(|s| s.id)
                    .collect()
            };
            for id in targets {
                updates.retain(|(existing, _)| *existing != id);
                updates.push((id, price));
            }
        }
        self.change(input.expected_revision, |editor| {
            let mut changed = 0;
            for (id, price) in &updates {
                let Some(current) = editor.project().stock_piece(*id) else {
                    continue;
                };
                let mut next = StockInput::from(current);
                next.price = *price;
                if editor.edit_stock(*id, next)? {
                    changed += 1;
                }
            }
            Ok((
                json!({ "updated": changed }),
                format!("Set {changed} price(s)."),
            ))
        })
    }

    pub fn duplicate_stock(&mut self, input: DuplicateStockInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Stock, &input.stock)?;
        let count = input.count.unwrap_or(1).clamp(1, 1000);
        self.change(input.expected_revision, |editor| {
            let mut aliases = Vec::new();
            for _ in 0..count {
                let copy = editor.duplicate_stock(id)?;
                aliases.push(editor.project().stock_alias(copy).map(str::to_owned));
            }
            Ok((
                json!({ "aliases": aliases }),
                format!("Added {count} copies."),
            ))
        })
    }

    pub fn delete_stock(&mut self, input: DeleteStockInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Stock, &input.stock)?;
        let alias = self
            .project()?
            .stock_alias(id)
            .unwrap_or_default()
            .to_owned();
        self.change(input.expected_revision, |editor| {
            let on_it: Vec<Uuid> = editor
                .project()
                .allocations
                .iter()
                .filter(|a| a.stock_id == id)
                .map(|a| a.board_id)
                .collect();
            if !on_it.is_empty() && input.unallocate_first {
                let mut session = SheetEditSession::begin(editor);
                for board in &on_it {
                    session.set_lock(*board, false)?;
                    session.unallocate(*board)?;
                }
                session.accept()?;
            }
            editor.delete_stock(id)?;
            Ok((json!({ "unallocated_boards": if input.unallocate_first { on_it.len() } else { 0 } }), format!("Deleted {alias}.")))
        })
    }

    pub fn reorder_stock(&mut self, input: ReorderStockInput) -> ServiceResult<Change<Value>> {
        let id = self.resolve(Kind::Stock, &input.stock)?;
        if input.rank == 0 {
            return Err(ServiceError::invalid("rank starts at 1"));
        }
        self.change(input.expected_revision, |editor| {
            editor.reorder_stock(id, input.rank - 1)?;
            let project = editor.project();
            let order: Vec<_> = project
                .ordered_stock()
                .iter()
                .map(|s| project.stock_alias(s.id))
                .collect();
            Ok((json!({ "order": order }), "Reordered stock.".to_owned()))
        })
    }

    pub fn suggest_sheets(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        let unit = project.display_unit;
        let rows: Vec<_> = suggest_sheets(project)
            .into_iter()
            .map(|s| {
                json!({
                    "material": object_name(project, s.material_id),
                    "thickness": LengthOut::new(s.thickness, unit),
                    "waiting_boards": s.boards.iter().map(|b| object_name(project, *b)).collect::<Vec<_>>(),
                    "blocked": s.blocked.iter().map(|(b, r)| json!({ "board": object_name(project, *b), "reason": unplaced_name(*r) })).collect::<Vec<_>>(),
                    "suggested_sheet": s.sheet.as_ref().map(|sheet| json!({
                        "name": sheet.name,
                        "length": LengthOut::new(sheet.length, unit),
                        "width": LengthOut::new(sheet.width, unit),
                        "grain": sheet_grain_name(sheet.grain),
                        "price": money_out(sheet.price),
                        "count": sheet.count,
                    })),
                })
            })
            .collect();
        Ok(
            json!({ "revision": project.revision, "suggestions": rows, "note": "No suggested_sheet means the material has no known sheet size: use create_stock." }),
        )
    }

    pub fn add_needed_sheets(
        &mut self,
        input: AddNeededSheetsInput,
    ) -> ServiceResult<Change<Value>> {
        let project = self.project()?;
        let material = input
            .material
            .as_ref()
            .map(|m| self.resolve(Kind::Material, m))
            .transpose()?;
        let price = input
            .price
            .as_ref()
            .map(|m| m.resolve(project.currency))
            .transpose()?;
        let suggestions: Vec<_> = suggest_sheets(project)
            .into_iter()
            .filter(|s| material.is_none_or(|m| s.material_id == m))
            .collect();
        let without_size: Vec<String> = suggestions
            .iter()
            .filter(|s| s.sheet.is_none() && s.waiting > 0)
            .map(|s| object_name(project, s.material_id))
            .collect();
        let mut change = self.change(input.expected_revision, |editor| {
            let mut added = Vec::new();
            let mut last = None;
            for suggestion in &suggestions {
                let Some(mut sheet) = suggestion.sheet.clone() else {
                    continue;
                };
                if material.is_some() {
                    if let Some(count) = input.count {
                        sheet.count = count;
                    }
                    if price.is_some() {
                        sheet.price = price;
                    }
                }
                if sheet.count == 0 {
                    continue;
                }
                let summary = editor.add_sheets_and_place(
                    suggestion.material_id,
                    suggestion.thickness,
                    &sheet,
                )?;
                added.push(json!({
                    "material": object_name(editor.project(), suggestion.material_id),
                    "thickness_mm": mm_f64(suggestion.thickness),
                    "count": sheet.count,
                    "size_mm": [mm_f64(sheet.length), mm_f64(sheet.width)],
                    "price_each": money_out(sheet.price),
                }));
                last = Some(summary);
            }
            let project = editor.project();
            let count = added.len();
            Ok((
                json!({
                    "added": added,
                    "placement": last.as_ref().map(|s| place_summary_json(project, s)),
                }),
                if count == 0 {
                    "No sheets were needed or known.".to_owned()
                } else {
                    format!("Added sheets for {count} material(s).")
                },
            ))
        })?;
        if !without_size.is_empty() {
            change.warnings.push(format!(
                "No standard sheet size for: {}. Declare sheets with create_stock.",
                without_size.join(", ")
            ));
        }
        Ok(change)
    }

    pub fn auto_place(&mut self, input: AutoPlaceInput) -> ServiceResult<Change<Value>> {
        let mode = match input.mode {
            PackModeName::FillGaps => PackMode::FillGaps,
            PackModeName::Replan => PackMode::Replan,
        };
        self.change(input.expected_revision, |editor| {
            let summary = editor.auto_place(mode)?;
            let project = editor.project();
            let text = format!(
                "{} board(s) placed or moved, {} waiting.",
                summary.placed,
                summary.unplaced.len()
            );
            Ok((place_summary_json(project, &summary), text))
        })
    }

    pub fn get_cut_plan(&self, input: CutPlanInput) -> ServiceResult<Value> {
        let project = self.project()?;
        let model = read_model(project)?;
        let unit = project.display_unit;
        let sheets: Vec<_> = model
            .pieces
            .iter()
            .filter(|p| p.is_used())
            .map(|p| {
                let mut row = piece_row(project, p);
                row["part_names"] =
                    json!(p.parts.iter().map(|x| x.name.clone()).collect::<Vec<_>>());
                if input.include_cuts {
                    row["cut_sequence"] = cut_sequence(p);
                }
                row
            })
            .collect();
        let waiting: Vec<_> = model
            .boards
            .iter()
            .filter(|d| d.status != Status::AllocatedValid)
            .map(|d| {
                let board = project.board(d.board_id);
                json!({
                    "board": board.map(|b| b.name.clone()),
                    "board_id": d.board_id,
                    "status": match d.status { Status::Unallocated => "unallocated", Status::Conflicted => "conflicted", Status::UnknownSearchBudget => "unknown", Status::AllocatedValid => "valid" },
                    "reasons": d.reasons.iter().map(|r| r.key()).collect::<Vec<_>>(),
                    "material": board.and_then(|b| project.material(b.material_id)).map(|m| m.name.clone()),
                })
            })
            .collect();
        let usage = model.usage_summary();
        Ok(json!({
            "revision": project.revision,
            "kerf": LengthOut::new(project.cutting_kerf, unit),
            "sheets": sheets,
            "unused_sheets": model.pieces.iter().filter(|p| !p.is_used()).map(|p| p.alias.clone()).collect::<Vec<_>>(),
            "boards_not_ready": waiting,
            "usage": {
                "purchased_pieces_used": usage.used_purchased_pieces,
                "owned_pieces_used": usage.consumed_owned_pieces,
                "cuts": usage.physical_cuts,
            },
            "estimate": {
                "material": money_out(model.estimate.material),
                "cutting": money_out(model.estimate.cutting),
                "total": money_out(model.estimate.total),
                "complete": matches!(model.estimate.feasibility, crate::cost_estimate::Feasibility::Verified),
                "cut_fee": money_out(project.cut_fee),
            },
        }))
    }

    pub fn get_diagnostics(&self) -> ServiceResult<Value> {
        let project = self.project()?;
        let model = read_model(project)?;
        let boards: Vec<_> = diagnose(project)
            .into_iter()
            .filter(|d| d.status != Status::AllocatedValid)
            .map(|d| json!({
                "board": object_name(project, d.board_id),
                "board_id": d.board_id,
                "status": match d.status { Status::Unallocated => "unallocated", Status::Conflicted => "conflicted", Status::UnknownSearchBudget => "unknown", Status::AllocatedValid => "valid" },
                "reasons": d.reasons.iter().map(|r| r.key()).collect::<Vec<_>>(),
            }))
            .collect();
        let unverified: Vec<_> = model
            .pieces
            .iter()
            .filter(|p| p.is_used() && p.proof.verified().is_none())
            .map(|p| json!({ "stock": p.alias, "status": proof_name(&p.proof) }))
            .collect();
        let unknown_prices: Vec<_> = model
            .pieces
            .iter()
            .filter(|p| {
                p.is_used()
                    && p.price.is_none()
                    && p.source == crate::domain::StockSource::ToPurchase
            })
            .map(|p| p.alias.clone())
            .collect();
        let hinges: Vec<_> = crate::hinge_installation::diagnose_all(project)
            .into_iter()
            .filter(|s| !s.issues.is_empty())
            .map(|s| json!({
                "hinge_id": s.id,
                "issues": s.issues.iter().map(crate::service::error::installation_issue_text).collect::<Vec<_>>(),
            }))
            .collect();
        let doors: Vec<_> = project
            .door_joints
            .iter()
            .filter(|j| crate::door_joint::needs_review(project, j))
            .map(|j| object_name(project, j.moving_root_id))
            .collect();
        let kerf_confirmed = project.confirmed_shop_kerf == Some(project.cutting_kerf);
        let cut_ready = boards.is_empty() && unverified.is_empty() && !project.boards.is_empty();
        let mut todo = Vec::new();
        if project.boards.is_empty() {
            todo.push("Create boards.".to_owned());
        }
        if !boards.is_empty() {
            todo.push("Some boards are not validly on a sheet: add stock (add_needed_sheets / create_stock) and run auto_place.".into());
        }
        if !unverified.is_empty() {
            todo.push("Some sheets have no valid cut sequence: run auto_place with mode replan, or edit_sheet.".into());
        }
        if !kerf_confirmed {
            todo.push("The shop kerf is not confirmed (set_project_settings confirm_shop_kerf) — needed for a shop-ready handoff.".into());
        }
        if !unknown_prices.is_empty() || project.cut_fee.is_none() {
            todo.push("Prices or the cut fee are unknown, so the cost is incomplete (set_stock_prices, set_project_settings cut_fee).".into());
        }
        if !hinges.is_empty() {
            todo.push("Some hinges have issues (list_hinges).".into());
        }
        if !doors.is_empty() {
            todo.push("Some doors need review: remove_door and create_door again.".into());
        }
        Ok(json!({
            "revision": project.revision,
            "ready_to_cut": cut_ready && kerf_confirmed,
            "boards_with_problems": boards,
            "sheets_without_valid_cuts": unverified,
            "kerf_confirmed": kerf_confirmed,
            "unknown_prices": unknown_prices,
            "cut_fee_known": project.cut_fee.is_some(),
            "hinge_issues": hinges,
            "doors_needing_review": doors,
            "todo": todo,
        }))
    }

    pub fn edit_sheet(&mut self, input: EditSheetInput) -> ServiceResult<Change<Value>> {
        if input.ops.is_empty() {
            return Err(ServiceError::invalid("ops is empty"));
        }
        // Resolve everything before opening the session.
        enum Op {
            Place(Uuid, Uuid, [Length; 2], bool),
            Unallocate(Uuid),
            Lock(Uuid, bool),
        }
        let mut ops = Vec::new();
        for op in &input.ops {
            ops.push(match op {
                SheetOp::Place {
                    board,
                    stock,
                    x,
                    y,
                    quarter_turn,
                } => Op::Place(
                    self.resolve(Kind::Board, board)?,
                    self.resolve(Kind::Stock, stock)?,
                    [
                        x.non_negative("x", input.allow_rounding)?,
                        y.non_negative("y", input.allow_rounding)?,
                    ],
                    *quarter_turn,
                ),
                SheetOp::Unallocate { board } => Op::Unallocate(self.resolve(Kind::Board, board)?),
                SheetOp::Lock { board, locked } => {
                    Op::Lock(self.resolve(Kind::Board, board)?, *locked)
                }
            });
        }
        let dry_run = input.dry_run;
        let editor = self.editor_mut()?;
        let before = editor.project().revision;
        if let Some(expected) = input.expected_revision
            && expected != before
        {
            return Err(ServiceError::new(
                ErrorCode::RevisionMismatch,
                format!("the project is at revision {before}, not {expected}"),
            ));
        }
        let mut session = SheetEditSession::begin(editor);
        for (index, op) in ops.iter().enumerate() {
            let result = match op {
                Op::Place(board, stock, origin, turn) => {
                    session.place(*board, *stock, *origin, *turn)
                }
                Op::Unallocate(board) => session.unallocate(*board),
                Op::Lock(board, locked) => session.set_lock(*board, *locked),
            };
            if let Err(e) = result {
                let mut error = ServiceError::from(e);
                error.message = format!("ops[{index}]: {}", error.message);
                return Err(error);
            }
        }
        let diagnostics = session.diagnostics();
        let sheets = sheet_diagnostics_json(session.preview(), &diagnostics);
        let valid = diagnostics
            .iter()
            .all(|d| matches!(d.status, SheetStatus::Verified(_)));
        if dry_run {
            session.cancel();
            return self.preview_only(
                json!({ "sheets": sheets, "valid": valid }),
                if valid {
                    "The layout is valid.".into()
                } else {
                    "The layout is not valid.".into()
                },
            );
        }
        match session.accept() {
            Ok(_) => {}
            Err(crate::commands::EditError::Command(SheetEditError::InvalidPlacement(_))) => {
                return Err(ServiceError::new(
                    ErrorCode::InvalidPlacement,
                    "the layout cannot be cut; nothing was changed",
                )
                .hint("See details.sheets: move parts apart (kerf included) or so a straight full-length cut sequence exists.")
                .details(json!({ "sheets": sheets })));
            }
            Err(e) => return Err(e.into()),
        }
        drop(session);
        let editor = self.editor()?;
        let after = editor.project().revision;
        Ok(crate::service::dto::Change {
            committed: true,
            changed: after != before,
            revision: after,
            dirty: editor.is_dirty(),
            undo_steps: (after - before) as usize,
            summary: format!("Applied {} sheet operation(s).", ops.len()),
            warnings: Vec::new(),
            result: json!({ "sheets": sheets }),
        })
    }

    pub fn optimize_cut_plan(&mut self, input: OptimizeInput) -> ServiceResult<Value> {
        let objective = match input.objective {
            ObjectiveName::LowestNewSpending => Objective::LowestNewSpending,
            ObjectiveName::FewestCuts => Objective::FewestCuts,
            ObjectiveName::LeastUnusedStockArea => Objective::LeastUnusedStockArea,
        };
        let seconds = input.max_seconds.unwrap_or(10.0).clamp(1.0, 60.0);
        let editor = self.editor()?;
        let revision = editor.project().revision;
        let mut worker = OptimizationWorker::start(
            editor,
            objective,
            SearchBudget {
                placements: 10_000,
                witness_states: 20_000,
                beam_width: 8,
            },
            Duration::from_secs_f64(seconds),
        );
        let result = worker.wait(Duration::from_secs_f64(seconds + 30.0))?;
        let project = self.project()?;
        let top = input.top.unwrap_or(3).clamp(1, 10);
        let describe = |c: &RankedCandidate| {
            let sheets: HashSet<Uuid> =
                c.candidate.allocations.iter().map(|a| a.stock_id).collect();
            let u = c.utilization;
            json!({
                "cuts": c.cuts,
                "sheets_used": sheets.iter().filter_map(|s| project.stock_alias(*s)).collect::<Vec<_>>(),
                "new_spending": money_out(c.new_spending),
                "part_area_percent": percent(Some((u.finished_part_area, u.used_stock_area))),
                "offcut_area_percent": percent(Some((u.recoverable_offcut_area, u.used_stock_area))),
                "reusable_offcuts": c.offcuts.len(),
            })
        };
        let candidates: Vec<_> = result
            .ranking
            .candidates
            .iter()
            .take(top)
            .enumerate()
            .map(|(i, c)| {
                let mut row = describe(c);
                row["index"] = json!(i);
                row["boards_moved"] = json!(result.placement_changes(i).map_or(0, |m| m.len()));
                row
            })
            .collect();
        let current = result.original_plan.as_ref().map(describe);
        let id = Uuid::new_v4();
        let empty = result.ranking.candidates.is_empty();
        let exhausted = result.exhausted;
        self.searches.push_back(StoredSearch {
            id,
            objective,
            result,
            revision,
        });
        while self.searches.len() > 4 {
            self.searches.pop_front();
        }
        let mut out = json!({
            "search_id": id,
            "revision": revision,
            "search_hit_limit": exhausted,
            "current_plan": current,
            "candidates": candidates,
        });
        if empty {
            out["note"] = json!(
                "No complete plan was found. Check get_diagnostics: every board needs a sheet of its material that it fits on; or allow more time."
            );
        } else {
            out["next"] = json!(
                "apply_optimization with this search_id and a candidate index (0 = best), before editing anything else."
            );
        }
        Ok(out)
    }

    pub fn apply_optimization(
        &mut self,
        input: ApplyOptimizationInput,
    ) -> ServiceResult<Change<Value>> {
        let id = uuid::Uuid::parse_str(input.search_id.trim())
            .map_err(|_| ServiceError::invalid("search_id is not a valid id"))?;
        let index = input.candidate.unwrap_or(0);
        let position = self
            .searches
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| {
                ServiceError::new(ErrorCode::StaleSearch, "that search is no longer available")
                    .hint("Run optimize_cut_plan again.")
            })?;
        let stored = self
            .searches
            .remove(position)
            .ok_or_else(|| ServiceError::internal("search vanished"))?;
        let moved = stored
            .result
            .placement_changes(index)
            .map_or(0, |m| m.len());
        let objective = stored.objective;
        let started = stored.revision;
        let change = self.change(input.expected_revision, |editor| {
            crate::optimization_worker::OptimizationWorker::apply(editor, &stored.result, index)?;
            Ok((
                json!({ "boards_moved": moved, "objective": format!("{objective:?}"), "searched_at_revision": started }),
                format!("Applied candidate {index}: {moved} board(s) moved."),
            ))
        });
        match change {
            Ok(change) => {
                self.searches.clear();
                Ok(change)
            }
            Err(e) => Err(e),
        }
    }
}
