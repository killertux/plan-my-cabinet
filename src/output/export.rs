//! Stable manufacturing input identity and historical output receipts.
//! Hashes identify content, not feasibility; the PDF writer and release gate are separate.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::allocation_diagnostics::{Reason, Status, diagnose};
use crate::cut_tree::{
    CutTree, Reconstruction, ReconstructionViolation, WitnessError, reconstruct_witness,
    validate_witness,
};
use crate::document_layout::{Document, FONT_METRICS_VERSION, LAYOUT_VERSION};
use crate::domain::{DomainError, HardwareKind, Project, StockSource};
use crate::hinge_installation::{
    InstallationIssue, InstallationReferences, diagnose as diagnose_hinge,
};
use crate::i18n::Language;
use crate::pdf_export::{PdfExportError, render_document_pdf, render_pdf};
use crate::persistence::{
    NoFailure, SaveError, SaveStages, atomic_write_new_with_stages, atomic_write_with_stages,
};
use crate::units::Length;
use crate::units::Unit;
use crate::workshop_document::{WorkshopDocumentError, build_workshop_document};
use uuid::Uuid as Id;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    pub language: Language,
    pub units: Unit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportMode {
    Draft,
    ShopReady,
}

/// These choices are portable packet settings; a missing value on an old
/// receipt means unknown, not "all included".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptSections {
    pub parts_and_costs: bool,
    pub sheets_and_cut_steps: bool,
    pub hinge_references: bool,
}

impl Default for ReceiptSections {
    fn default() -> Self {
        Self {
            parts_and_costs: true,
            sheets_and_cut_steps: true,
            hinge_references: true,
        }
    }
}

pub const RECEIPT_METADATA_VERSION: u16 = 1;
const CURRENT_FINGERPRINT_VERSION: u16 = 4;
pub const MAX_COMPARISON_ENTRIES: usize = 4096;
pub const MAX_COMPARISON_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComparisonKind {
    Project,
    Material,
    Board,
    Stock,
    Allocation,
    Catalog,
    Hardware,
    Installation,
    Joint,
}

/// Historical values are deliberately independent of today's entities: a
/// deleted board or stock piece must still be explainable later.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonEntry {
    pub kind: ComparisonKind,
    pub id: Uuid,
    pub name: String,
    pub facts: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonBaseline {
    pub version: u16,
    pub entries: Vec<ComparisonEntry>,
    pub cutting_kerf_um: i64,
    pub cut_fee_minor: Option<i64>,
}

impl ComparisonBaseline {
    pub fn from_project(project: &Project) -> Result<Self, ExportError> {
        let mut normalized = project.clone();
        normalized
            .assign_missing_stock_aliases()
            .map_err(|_| ExportError::InvalidMetadata)?;
        let project = &normalized;
        let mut entries = Vec::new();
        let mut add = |kind, id, name: &str, facts: BTreeMap<String, String>| {
            entries.push(ComparisonEntry {
                kind,
                id,
                name: name.into(),
                facts,
            });
        };
        add(
            ComparisonKind::Project,
            project.id,
            &project.name,
            BTreeMap::from([
                ("currency".into(), format!("{:?}", project.currency)),
                (
                    "kerf_um".into(),
                    project.cutting_kerf.micrometres().to_string(),
                ),
                ("cut_fee".into(), format!("{:?}", project.cut_fee)),
            ]),
        );
        for material in &project.materials {
            add(
                ComparisonKind::Material,
                material.id,
                &material.name,
                BTreeMap::from([
                    (
                        "thickness_um".into(),
                        material.default_thickness.micrometres().to_string(),
                    ),
                    ("grain".into(), format!("{:?}", material.default_grain)),
                ]),
            );
        }
        for board in &project.boards {
            add(
                ComparisonKind::Board,
                board.id,
                &board.name,
                BTreeMap::from([
                    ("material".into(), board.material_id.to_string()),
                    ("length_um".into(), board.length.micrometres().to_string()),
                    ("width_um".into(), board.width.micrometres().to_string()),
                    (
                        "thickness_um".into(),
                        board.thickness.micrometres().to_string(),
                    ),
                    (
                        "grain".into(),
                        format!(
                            "{:?}",
                            project
                                .materials
                                .iter()
                                .find(|material| material.id == board.material_id)
                                .map(|material| board.effective_grain(material))
                        ),
                    ),
                ]),
            );
        }
        for stock in &project.stock {
            add(
                ComparisonKind::Stock,
                stock.id,
                &stock.name,
                BTreeMap::from([
                    ("material".into(), stock.material_id.to_string()),
                    ("length_um".into(), stock.length.micrometres().to_string()),
                    ("width_um".into(), stock.width.micrometres().to_string()),
                    (
                        "thickness_um".into(),
                        stock.thickness.micrometres().to_string(),
                    ),
                    ("grain".into(), format!("{:?}", stock.grain)),
                    ("source".into(), format!("{:?}", stock.source)),
                    ("priority".into(), stock.priority.to_string()),
                    (
                        "alias".into(),
                        project.stock_alias(stock.id).unwrap_or("").into(),
                    ),
                    ("price".into(), format!("{:?}", stock.price)),
                    (
                        "trim_um".into(),
                        format!("{:?}", stock.trim.map(|v| v.micrometres())),
                    ),
                ]),
            );
        }
        for allocation in &project.allocations {
            add(
                ComparisonKind::Allocation,
                allocation.id,
                "",
                BTreeMap::from([
                    ("board".into(), allocation.board_id.to_string()),
                    ("stock".into(), allocation.stock_id.to_string()),
                    (
                        "origin_um".into(),
                        format!("{:?}", allocation.origin.map(|v| v.micrometres())),
                    ),
                    ("quarter_turn".into(), allocation.quarter_turn.to_string()),
                ]),
            );
        }
        for catalog in &project.catalog {
            add(
                ComparisonKind::Catalog,
                catalog.id,
                &catalog.name,
                BTreeMap::from([
                    ("product".into(), catalog.product_id.clone()),
                    ("plate".into(), format!("{:?}", catalog.plate_id)),
                    ("revision".into(), catalog.revision.clone()),
                    ("source".into(), catalog.source.clone()),
                    (
                        "dimensions".into(),
                        format!(
                            "{:?}",
                            catalog
                                .installation_dimensions
                                .iter()
                                .collect::<BTreeMap<_, _>>()
                        ),
                    ),
                    ("verified".into(), format!("{:?}", catalog.verified_hinge)),
                ]),
            );
        }
        for hardware in &project.hardware {
            add(
                ComparisonKind::Hardware,
                hardware.id,
                &hardware.name,
                BTreeMap::from([("kind".into(), format!("{:?}", hardware.kind))]),
            );
        }
        for installation in &project.hinge_installations {
            add(
                ComparisonKind::Installation,
                installation.id,
                "",
                BTreeMap::from([("reference".into(), format!("{installation:?}"))]),
            );
        }
        for joint in &project.door_joints {
            add(
                ComparisonKind::Joint,
                joint.id,
                "",
                BTreeMap::from([("reference".into(), format!("{joint:?}"))]),
            );
        }
        entries.sort_by_key(|entry| (entry.id, entry.kind as u8));
        let baseline = Self {
            version: RECEIPT_METADATA_VERSION,
            entries,
            cutting_kerf_um: project.cutting_kerf.micrometres(),
            cut_fee_minor: project.cut_fee.map(|fee| fee.minor_units()),
        };
        if !baseline.is_valid() {
            return Err(ExportError::InvalidMetadata);
        }
        Ok(baseline)
    }

    pub fn is_valid(&self) -> bool {
        if self.version != RECEIPT_METADATA_VERSION
            || self.entries.len() > MAX_COMPARISON_ENTRIES
            || self.cutting_kerf_um <= 0
            || self.cut_fee_minor.is_some_and(|fee| fee < 0)
        {
            return false;
        }
        let mut seen = HashSet::new();
        self.entries.iter().all(|entry| {
            seen.insert((entry.kind, entry.id))
                && entry.name.len() <= 512
                && entry.facts.len() <= 32
                && entry
                    .facts
                    .iter()
                    .all(|(key, value)| !key.is_empty() && key.len() <= 64 && value.len() <= 2048)
        }) && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= MAX_COMPARISON_BYTES)
    }

    /// Identity-level changes backed by stored before values, including removals.
    pub fn changed_entries(&self, project: &Project) -> Result<Vec<ComparisonChange>, ExportError> {
        let current = Self::from_project(project)?;
        let before: BTreeMap<_, _> = self
            .entries
            .iter()
            .map(|entry| ((entry.kind as u8, entry.id), entry))
            .collect();
        let after: BTreeMap<_, _> = current
            .entries
            .iter()
            .map(|entry| ((entry.kind as u8, entry.id), entry))
            .collect();
        let mut keys: Vec<_> = before.keys().chain(after.keys()).copied().collect();
        keys.sort_unstable();
        keys.dedup();
        Ok(keys
            .into_iter()
            .filter_map(|key| {
                let previous = before.get(&key).copied();
                let current = after.get(&key).copied();
                (previous != current).then(|| ComparisonChange {
                    kind: previous.or(current).expect("change has a side").kind,
                    id: key.1,
                    previous: previous.cloned(),
                    current: current.cloned(),
                })
            })
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonChange {
    pub kind: ComparisonKind,
    pub id: Uuid,
    pub previous: Option<ComparisonEntry>,
    pub current: Option<ComparisonEntry>,
}

/// An issue refers to a physical board or sheet where possible; the UI can locate it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportIssue {
    Board {
        id: Id,
        name: String,
        stock_id: Option<Id>,
        reasons: Vec<Reason>,
    },
    Sheet {
        id: Id,
        name: String,
        reason: SheetIssue,
    },
    KerfUnconfirmed(Length),
    UnknownPrice {
        stock_id: Option<Id>,
    },
    Hardware {
        id: Id,
        name: String,
        reason: HardwareIssue,
    },
    Installation {
        id: Id,
        name: String,
        reason: InstallationIssue,
    },
    JointNeedsReview {
        id: Id,
        installation_id: Id,
    },
    InvalidWood(DomainError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetIssue {
    BudgetExhausted,
    RuleViolation(ReconstructionViolation),
    InvalidWitness(WitnessError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HardwareIssue {
    MissingCatalog(Id),
    UnverifiedInstallation,
    InvalidReference,
}

#[derive(Clone, Debug)]
pub struct ExportInstallationGuidance {
    pub id: Id,
    pub door_name: String,
    pub mount_name: String,
    pub cup_edge_setback: Length,
    pub overlay: Length,
    pub references: InstallationReferences,
}

#[derive(Clone, Debug)]
pub struct PreparedExport {
    pub snapshot: ExportSnapshot,
    pub mode: ExportMode,
    pub wood_issues: Vec<ExportIssue>,
    pub notices: Vec<ExportIssue>,
    /// Renderers must omit numeric installation diagrams for these hardware IDs.
    pub withheld_installation_guidance: Vec<Id>,
    pub installation_guidance: Vec<ExportInstallationGuidance>,
    /// One fresh, independently checked full-span sequence for each used stock.
    pub witnesses: Vec<(Id, CutTree)>,
    /// PDF/layout adapters must repeat this on every draft layout page.
    pub preview_watermark: Option<&'static str>,
}

/// The complete identity of a reviewed layout. A presentation-only edit changes
/// `revision` (and requires a fresh preview) without changing the manufacturing
/// fingerprint used to assess an already-written receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewedPacketKey {
    pub project_id: Uuid,
    pub revision: u64,
    pub fingerprint: ManufacturingFingerprint,
    pub mode: ExportMode,
    pub settings: ExportSettings,
    pub sections: ReceiptSections,
    pub layout_version: u32,
    pub font_metrics_version: u32,
}

impl ReviewedPacketKey {
    pub fn for_project(
        project: &Project,
        mode: ExportMode,
        settings: ExportSettings,
        sections: ReceiptSections,
    ) -> Result<Self, DomainError> {
        // Draft packets can deliberately contain invalid allocations or
        // hardware references; matching their source must not require a
        // shop-ready project.
        let mut normalized = project.clone();
        normalized.assign_missing_stock_aliases()?;
        Ok(Self {
            project_id: normalized.id,
            revision: normalized.revision,
            fingerprint: fingerprint(&normalized),
            mode,
            settings,
            sections,
            layout_version: LAYOUT_VERSION,
            font_metrics_version: FONT_METRICS_VERSION,
        })
    }
}

#[derive(Debug)]
pub enum ReviewPreparationError {
    Cancelled,
    Blocked(ShopReadyBlocked),
    Document(WorkshopDocumentError),
}

/// One atomically prepared, read-only pairing of validated evidence and its
/// positioned pages. No public constructor accepts an independently built PDF
/// or document, so a picker can only write the pages actually reviewed.
pub struct ReviewedPacket {
    key: ReviewedPacketKey,
    prepared: PreparedExport,
    document: Document,
}

impl ReviewedPacket {
    pub fn prepare(
        project: &Project,
        mode: ExportMode,
        settings: ExportSettings,
        sections: ReceiptSections,
    ) -> Result<Self, ReviewPreparationError> {
        Self::prepare_cancellable(project, mode, settings, sections, || false)
    }

    pub fn prepare_cancellable(
        project: &Project,
        mode: ExportMode,
        settings: ExportSettings,
        sections: ReceiptSections,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, ReviewPreparationError> {
        if cancelled() {
            return Err(ReviewPreparationError::Cancelled);
        }
        let mut prepared =
            prepare_export(project, settings, mode).map_err(ReviewPreparationError::Blocked)?;
        if cancelled() {
            return Err(ReviewPreparationError::Cancelled);
        }
        // Prepare already normalized the source, including historical aliases.
        // Freeze section metadata *before* layout so neither can diverge.
        prepared.snapshot.sections = Some(sections);
        prepared.snapshot.layout_version = LAYOUT_VERSION as u16;
        let key = ReviewedPacketKey {
            project_id: prepared.snapshot.project_id(),
            revision: prepared.snapshot.revision(),
            fingerprint: prepared.snapshot.fingerprint().clone(),
            mode,
            settings,
            sections,
            layout_version: LAYOUT_VERSION,
            font_metrics_version: FONT_METRICS_VERSION,
        };
        let document = build_workshop_document(&prepared, sections)
            .map_err(ReviewPreparationError::Document)?;
        if cancelled() {
            return Err(ReviewPreparationError::Cancelled);
        }
        debug_assert_eq!(document.layout_version, key.layout_version);
        debug_assert_eq!(document.font_metrics_version, key.font_metrics_version);
        Ok(Self {
            key,
            prepared,
            document,
        })
    }

    pub fn key(&self) -> &ReviewedPacketKey {
        &self.key
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn snapshot(&self) -> &ExportSnapshot {
        &self.prepared.snapshot
    }

    pub fn wood_issues(&self) -> &[ExportIssue] {
        &self.prepared.wood_issues
    }

    pub fn notices(&self) -> &[ExportIssue] {
        &self.prepared.notices
    }

    pub fn matches_source(
        &self,
        project: &Project,
        mode: ExportMode,
        settings: ExportSettings,
        sections: ReceiptSections,
    ) -> bool {
        ReviewedPacketKey::for_project(project, mode, settings, sections)
            .is_ok_and(|current| current == self.key)
    }
}

#[derive(Clone, Debug)]
pub struct ShopReadyBlocked {
    pub issues: Vec<ExportIssue>,
}

/// Always prepares a draft; shop-ready requires a fresh proof at the current kerf.
pub fn prepare_export(
    project: &Project,
    settings: ExportSettings,
    mode: ExportMode,
) -> Result<PreparedExport, ShopReadyBlocked> {
    prepare_export_with_budget(
        project,
        settings,
        mode,
        crate::allocation_diagnostics::WITNESS_BUDGET,
    )
}

pub fn prepare_export_with_budget(
    project: &Project,
    settings: ExportSettings,
    mode: ExportMode,
    budget: usize,
) -> Result<PreparedExport, ShopReadyBlocked> {
    // A legacy in-memory project may lack aliases, but an exhausted counter
    // cannot produce a truthful PDF/receipt pair. Resolve before any hashing.
    let mut normalized = project.clone();
    if let Err(error) = normalized.assign_missing_stock_aliases() {
        return Err(ShopReadyBlocked {
            issues: vec![ExportIssue::InvalidWood(error)],
        });
    }
    let project = &normalized;
    let mut wood_issues = Vec::new();
    // Validate ordinary wood records independently from hardware and draft allocations.
    let mut wood = project.clone();
    wood.hardware.clear();
    wood.hinge_installations.clear();
    wood.door_joints.clear();
    wood.catalog.clear();
    wood.allocations.clear();
    if let Err(error) = wood.validate() {
        wood_issues.push(ExportIssue::InvalidWood(error));
    }
    if project.confirmed_shop_kerf != Some(project.cutting_kerf) {
        wood_issues.push(ExportIssue::KerfUnconfirmed(project.cutting_kerf));
    }
    for diagnostic in diagnose(project) {
        if diagnostic.status != Status::AllocatedValid {
            let board = project
                .boards
                .iter()
                .find(|b| b.id == diagnostic.board_id)
                .expect("diagnosed board");
            wood_issues.push(ExportIssue::Board {
                id: board.id,
                name: board.name.clone(),
                stock_id: project
                    .allocations
                    .iter()
                    .find(|a| a.board_id == board.id)
                    .map(|a| a.stock_id),
                reasons: diagnostic.reasons,
            });
        }
    }
    let mut witnesses = Vec::new();
    for stock in &project.stock {
        if !project.allocations.iter().any(|a| a.stock_id == stock.id) {
            continue;
        }
        match reconstruct_witness(project, stock.id, project.cutting_kerf, budget) {
            Reconstruction::Verified { tree, accounting } => {
                match validate_witness(&tree, project, stock.id) {
                    Ok(checked) if checked == accounting => witnesses.push((stock.id, tree)),
                    Ok(_) => wood_issues.push(ExportIssue::Sheet {
                        id: stock.id,
                        name: stock.name.clone(),
                        reason: SheetIssue::InvalidWitness(WitnessError::AreaMismatch(accounting)),
                    }),
                    Err(reason) => wood_issues.push(ExportIssue::Sheet {
                        id: stock.id,
                        name: stock.name.clone(),
                        reason: SheetIssue::InvalidWitness(reason),
                    }),
                }
            }
            Reconstruction::BudgetExhausted => wood_issues.push(ExportIssue::Sheet {
                id: stock.id,
                name: stock.name.clone(),
                reason: SheetIssue::BudgetExhausted,
            }),
            Reconstruction::RuleViolation(reason) => wood_issues.push(ExportIssue::Sheet {
                id: stock.id,
                name: stock.name.clone(),
                reason: SheetIssue::RuleViolation(reason),
            }),
        }
    }
    let mut notices = Vec::new();
    if project.cut_fee.is_none() {
        notices.push(ExportIssue::UnknownPrice { stock_id: None });
    }
    for stock in &project.stock {
        if stock.source == StockSource::ToPurchase
            && stock.price.is_none()
            && project.allocations.iter().any(|a| a.stock_id == stock.id)
        {
            notices.push(ExportIssue::UnknownPrice {
                stock_id: Some(stock.id),
            });
        }
    }
    for hardware in &project.hardware {
        if let HardwareKind::Catalog { catalog_id } = hardware.kind {
            let reason = match project.catalog.iter().find(|c| c.id == catalog_id) {
                None => HardwareIssue::MissingCatalog(catalog_id),
                Some(entry)
                    if entry
                        .installation_dimensions
                        .values()
                        .any(|v| v.micrometres() < 0) =>
                {
                    HardwareIssue::InvalidReference
                }
                Some(entry) if !crate::hardware_catalog::is_verified(entry) => {
                    HardwareIssue::UnverifiedInstallation
                }
                Some(_) => continue,
            };
            notices.push(ExportIssue::Hardware {
                id: hardware.id,
                name: hardware.name.clone(),
                reason,
            });
        }
    }
    let mut withheld_installation_guidance: Vec<_> = notices
        .iter()
        .filter_map(|issue| match issue {
            ExportIssue::Hardware { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    let mut installation_guidance = Vec::new();
    for installation in &project.hinge_installations {
        let status = diagnose_hinge(project, installation);
        let door_name = project
            .boards
            .iter()
            .find(|b| b.id == installation.door_board_id)
            .map_or_else(
                || installation.door_board_id.to_string(),
                |b| b.name.clone(),
            );
        let mount_name = project
            .boards
            .iter()
            .find(|b| b.id == installation.mounting_board_id)
            .map_or_else(
                || installation.mounting_board_id.to_string(),
                |b| b.name.clone(),
            );
        for reason in status.issues.iter().copied() {
            notices.push(ExportIssue::Installation {
                id: installation.id,
                name: door_name.clone(),
                reason,
            });
        }
        let joint = project
            .door_joints
            .iter()
            .find(|j| j.hinge_installation_ids.contains(&installation.id));
        let coherent = joint.is_some_and(|j| !crate::door_joint::needs_review(project, j));
        if !coherent {
            notices.push(ExportIssue::JointNeedsReview {
                id: joint.map_or(installation.id, |j| j.id),
                installation_id: installation.id,
            });
        }
        if status.issues.is_empty()
            && coherent
            && let Some(references) = status.references
        {
            installation_guidance.push(ExportInstallationGuidance {
                id: installation.id,
                door_name,
                mount_name,
                cup_edge_setback: installation.cup_edge_setback,
                overlay: installation.overlay,
                references,
            });
            continue;
        }
        withheld_installation_guidance.push(installation.id);
    }
    if mode == ExportMode::ShopReady && !wood_issues.is_empty() {
        return Err(ShopReadyBlocked {
            issues: wood_issues,
        });
    }
    Ok(PreparedExport {
        snapshot: ExportSnapshot {
            project: project.clone(),
            fingerprint: fingerprint(project),
            settings,
            mode: Some(mode),
            sections: Some(ReceiptSections::default()),
            layout_version: 1,
        },
        mode,
        wood_issues,
        notices,
        withheld_installation_guidance,
        installation_guidance,
        witnesses,
        preview_watermark: (mode == ExportMode::Draft).then_some("DRAFT / NOT FOR CUTTING"),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManufacturingFingerprint {
    pub wood: String,
    pub packet: String,
}

fn sorted<T>(items: &[T], id: impl Fn(&T) -> Uuid, value: impl Fn(&T) -> Value) -> Vec<Value> {
    let mut items: Vec<_> = items.iter().collect();
    items.sort_by_key(|item| id(item));
    items.into_iter().map(value).collect()
}

fn digest(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("JSON value"))
    )
}

/// Canonical, version-tagged input projection. Pose, parent, visibility, editing
/// locks, unused catalog entries and record history cannot affect shop content.
pub fn fingerprint(project: &Project) -> ManufacturingFingerprint {
    fingerprint_version(project, CURRENT_FINGERPRINT_VERSION)
}

/// Version 2 is the pre-alias projection used by historical receipts (including
/// records without an explicit version). Keep its exact JSON shape and tags.
fn fingerprint_version(project: &Project, version: u16) -> ManufacturingFingerprint {
    let materials = sorted(
        &project.materials,
        |m| m.id,
        |m| json!([m.id, m.name, m.default_thickness, m.default_grain]),
    );
    let boards = sorted(
        &project.boards,
        |b| b.id,
        |b| {
            let material = project.materials.iter().find(|m| m.id == b.material_id);
            json!([
                b.id,
                b.name,
                b.material_id,
                b.length,
                b.width,
                b.thickness,
                material.map(|m| b.effective_grain(m))
            ])
        },
    );
    let stock = sorted(
        &project.stock,
        |s| s.id,
        |s| {
            json!([
                s.id,
                s.name,
                s.material_id,
                s.length,
                s.width,
                s.thickness,
                s.grain,
                s.source,
                s.price,
                s.priority,
                s.trim
            ])
        },
    );
    let allocations = sorted(
        &project.allocations,
        |a| a.id,
        |a| json!([a.id, a.board_id, a.stock_id, a.origin, a.quarter_turn]),
    );
    let legacy_wood = json!([
        "wood-v1",
        project.id,
        project.name,
        project.currency,
        project.cutting_kerf,
        project.cut_fee,
        materials,
        boards,
        stock,
        allocations
    ]);
    let hardware = sorted(
        &project.hardware,
        |h| h.id,
        |h| {
            let kind = match &h.kind {
                HardwareKind::Placeholder { dimensions } => json!(["placeholder", dimensions]),
                HardwareKind::Catalog { catalog_id } => {
                    let entry = project.catalog.iter().find(|c| c.id == *catalog_id);
                    json!([
                        "catalog",
                        catalog_id,
                        entry.map(|c| json!([
                            c.name,
                            c.product_id,
                            c.plate_id,
                            c.source,
                            c.revision,
                            c.installation_dimensions
                                .iter()
                                .collect::<std::collections::BTreeMap<_, _>>()
                        ]))
                    ])
                }
            };
            json!([h.id, h.name, kind])
        },
    );
    let installations = sorted(
        &project.hinge_installations,
        |i| i.id,
        |i| {
            let entry = project.catalog.iter().find(|c| c.id == i.catalog_id);
            json!([
                i,
                entry.map(|c| json!([
                    c.name,
                    c.product_id,
                    c.plate_id,
                    c.source,
                    c.revision,
                    c.verified_hinge
                ]))
            ])
        },
    );
    let joints = sorted(
        &project.door_joints,
        |j| j.id,
        |j| {
            json!([
                j.id,
                j.moving_root_id,
                j.mounting_board_id,
                j.hinge_installation_ids
            ])
        },
    );
    // The wood cut projection deliberately ignores scene poses. A confirmed
    // mechanical axis, however, must be reviewed when its moving root drifts
    // (including movement inherited from an ancestor assembly).
    let joint_poses = sorted(
        &project.door_joints,
        |j| j.id,
        |j| {
            json!([
                j.id,
                crate::assembly_edit::world_pose(project, j.moving_root_id).ok()
            ])
        },
    );
    let legacy_packet = json!([
        "packet-v2",
        legacy_wood,
        hardware,
        installations,
        joints,
        joint_poses
    ]);
    if version == 2 {
        return ManufacturingFingerprint {
            wood: digest(&legacy_wood),
            packet: digest(&legacy_packet),
        };
    }
    // Stock aliases are printed alongside physical-piece references. They must
    // participate in the new wood and packet hashes even if dimensions do not
    // change. Legacy files receive the same deterministic aliases as on load.
    let normalized = if project.stock_aliases.len() == project.stock.len() {
        None
    } else {
        let mut copy = project.clone();
        copy.assign_missing_stock_aliases().ok().map(|_| copy)
    };
    let normalized = normalized.as_ref().unwrap_or(project);
    let aliases = sorted(
        &normalized.stock,
        |s| s.id,
        |s| json!([s.id, normalized.stock_alias(s.id)]),
    );
    let wood = json!(["wood-v3", legacy_wood, aliases]);
    let v3 = ManufacturingFingerprint {
        wood: digest(&wood),
        packet: digest(&json!(["packet-v3", wood, legacy_packet])),
    };
    if version == 3 {
        return v3;
    }
    // Only catalogs actually used by hardware are printed in the hardware
    // section. Installation catalogs already participate in the v3 projection.
    let hardware_guidance = sorted(
        &project.hardware,
        |h| h.id,
        |h| match h.kind {
            HardwareKind::Catalog { catalog_id } => json!([
                h.id,
                project
                    .catalog
                    .iter()
                    .find(|c| c.id == catalog_id)
                    .map(|c| &c.verified_hinge)
            ]),
            HardwareKind::Placeholder { .. } => json!([h.id, Value::Null]),
        },
    );
    ManufacturingFingerprint {
        wood: v3.wood,
        packet: digest(&json!([
            "packet-v4",
            v3.packet,
            hardware_guidance,
            project.confirmed_shop_kerf == Some(project.cutting_kerf)
        ])),
    }
}

/// Owned, validated committed input. Only read access is exposed to a renderer.
#[derive(Clone, Debug)]
pub struct ExportSnapshot {
    project: Project,
    fingerprint: ManufacturingFingerprint,
    settings: ExportSettings,
    mode: Option<ExportMode>,
    sections: Option<ReceiptSections>,
    layout_version: u16,
}

impl ExportSnapshot {
    pub fn new(project: &Project, settings: ExportSettings) -> Result<Self, DomainError> {
        project.validate()?;
        let mut normalized = project.clone();
        normalized.assign_missing_stock_aliases()?;
        Ok(Self {
            fingerprint: fingerprint(&normalized),
            project: normalized,
            settings,
            mode: Some(ExportMode::Draft),
            sections: Some(ReceiptSections::default()),
            layout_version: 1,
        })
    }

    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn project_id(&self) -> Uuid {
        self.project.id
    }
    pub fn revision(&self) -> u64 {
        self.project.revision
    }
    pub fn fingerprint(&self) -> &ManufacturingFingerprint {
        &self.fingerprint
    }
    pub fn settings(&self) -> ExportSettings {
        self.settings
    }

    pub fn mode(&self) -> Option<ExportMode> {
        self.mode
    }
    pub fn sections(&self) -> Option<ReceiptSections> {
        self.sections
    }
}

/// A receipt for bytes already written to a destination. Historical records are
/// metadata, never evidence that the current allocations are shop-ready.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRecord {
    pub project_id: Uuid,
    pub revision: u64,
    pub wood_sha256: String,
    pub packet_sha256: String,
    pub settings: ExportSettings,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "is_unknown_date")]
    pub completed_unix_ms: u64,
    pub file_sha256: String,
    #[serde(flatten)]
    pub metadata: Box<ReceiptMetadata>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ExportMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<ReceiptSections>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock_aliases: Option<BTreeMap<Uuid, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison_baseline: Option<ComparisonBaseline>,
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn is_unknown_date(date: &u64) -> bool {
    *date == 0
}

impl ExportRecord {
    pub(crate) fn is_valid(&self) -> bool {
        !self.path.as_os_str().is_empty()
            && self.completed_unix_ms <= 253_402_300_799_999
            && valid_hash(&self.wood_sha256)
            && valid_hash(&self.packet_sha256)
            && valid_hash(&self.file_sha256)
            && self
                .metadata
                .metadata_version
                .is_none_or(|v| v == RECEIPT_METADATA_VERSION)
            && self
                .metadata
                .fingerprint_version
                .is_none_or(|v| (2..=CURRENT_FINGERPRINT_VERSION).contains(&v))
            && self
                .metadata
                .layout_version
                .is_none_or(|v| (1..=LAYOUT_VERSION as u16).contains(&v))
            && self.metadata.stock_aliases.as_ref().is_none_or(|aliases| {
                aliases.len() <= MAX_COMPARISON_ENTRIES && {
                    let mut names = HashSet::new();
                    aliases.values().all(|alias| {
                        let number = alias.strip_prefix('S').or_else(|| alias.strip_prefix('O'));
                        number.is_some_and(|n| {
                            n.parse::<u64>().is_ok_and(|v| v > 0 && v.to_string() == n)
                        }) && names.insert(alias)
                    })
                }
            })
            && self
                .metadata
                .comparison_baseline
                .as_ref()
                .is_none_or(ComparisonBaseline::is_valid)
            && (self.metadata.metadata_version.is_none()
                || (self.metadata.mode.is_some()
                    && self.metadata.sections.is_some()
                    && self.metadata.fingerprint_version.is_some()
                    && self.metadata.layout_version.is_some()
                    && self.metadata.stock_aliases.is_some()
                    && self.metadata.comparison_baseline.is_some()))
            && (self.metadata.metadata_version.is_some()
                || (self.metadata.mode.is_none()
                    && self.metadata.sections.is_none()
                    && self.metadata.fingerprint_version.is_none()
                    && self.metadata.layout_version.is_none()
                    && self.metadata.stock_aliases.is_none()
                    && self.metadata.comparison_baseline.is_none()))
    }

    pub fn completed_at(&self) -> Option<u64> {
        (self.completed_unix_ms != 0).then_some(self.completed_unix_ms)
    }

    pub fn changed_entries(
        &self,
        project: &Project,
    ) -> Result<Option<Vec<ComparisonChange>>, ExportError> {
        self.metadata
            .comparison_baseline
            .as_ref()
            .map(|baseline| baseline.changed_entries(project))
            .transpose()
    }

    pub(crate) fn verify_completed(
        snapshot: &ExportSnapshot,
        path: &Path,
        time: u64,
        hash: &str,
    ) -> Result<Self, ExportError> {
        if !valid_hash(hash) {
            return Err(ExportError::InvalidHash);
        }
        if time > 253_402_300_799_999 {
            return Err(ExportError::InvalidMetadata);
        }
        let baseline = snapshot
            .mode
            .map(|_| ComparisonBaseline::from_project(&snapshot.project))
            .transpose()?;
        let aliases = snapshot
            .mode
            .map(|_| {
                let mut project = snapshot.project.clone();
                project
                    .assign_missing_stock_aliases()
                    .map_err(|_| ExportError::InvalidMetadata)?;
                Ok::<_, ExportError>(project.stock_aliases)
            })
            .transpose()?;
        let bytes = fs::read(path).map_err(ExportError::Io)?;
        if bytes.is_empty() || format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(ExportError::FileMismatch);
        }
        let record = Self {
            project_id: snapshot.project_id(),
            revision: snapshot.revision(),
            wood_sha256: snapshot.fingerprint.wood.clone(),
            packet_sha256: snapshot.fingerprint.packet.clone(),
            settings: snapshot.settings,
            path: path.to_path_buf(),
            completed_unix_ms: time,
            file_sha256: hash.to_owned(),
            metadata: Box::new(ReceiptMetadata {
                metadata_version: snapshot.mode.map(|_| RECEIPT_METADATA_VERSION),
                mode: snapshot.mode,
                sections: snapshot.sections,
                fingerprint_version: snapshot.mode.map(|_| CURRENT_FINGERPRINT_VERSION),
                layout_version: snapshot.mode.map(|_| snapshot.layout_version),
                stock_aliases: aliases,
                comparison_baseline: baseline,
            }),
        };
        if !record.is_valid() {
            return Err(ExportError::InvalidMetadata);
        }
        Ok(record)
    }
}

#[derive(Debug)]
pub enum ExportError {
    Io(std::io::Error),
    InvalidHash,
    FileMismatch,
    WrongProject,
    InvalidMetadata,
}

/// A destination must be explicitly approved when it already exists. No receipt
/// is created here: the caller verifies the committed bytes against the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overwrite {
    Decline,
    Confirm,
}

#[derive(Debug)]
pub enum OutputError {
    Cancelled,
    PreparationBlocked,
    OverwriteRequired,
    Pdf(PdfExportError),
    Write(SaveError),
    Verify(ExportError),
}

/// Render an owned snapshot and commit it in the destination directory. A
/// post-rename directory-sync error is distinct: the new bytes may be visible.
pub fn write_pdf(
    prepared: &PreparedExport,
    destination: Option<&Path>,
    overwrite: Overwrite,
) -> Result<ExportRecord, OutputError> {
    write_pdf_cancellable(prepared, destination, overwrite, || false)
}

pub fn write_pdf_cancellable(
    prepared: &PreparedExport,
    destination: Option<&Path>,
    overwrite: Overwrite,
    cancelled: impl Fn() -> bool,
) -> Result<ExportRecord, OutputError> {
    write_pdf_with_stages(prepared, destination, overwrite, cancelled, &NoFailure)
}

/// Write precisely the pages returned by `ReviewedPacket::document`. The
/// destination picker may complete later; no preparation or layout occurs here.
/// The caller can use `matches_source` to require a fresh review before opening
/// the picker. A stale packet still identifies its own historical source.
pub fn write_reviewed_pdf(
    packet: &ReviewedPacket,
    destination: Option<&Path>,
    overwrite: Overwrite,
    cancelled: impl Fn() -> bool,
) -> Result<ExportRecord, OutputError> {
    write_reviewed_pdf_with_stages(packet, destination, overwrite, cancelled, &NoFailure)
}

fn write_reviewed_pdf_with_stages(
    packet: &ReviewedPacket,
    destination: Option<&Path>,
    overwrite: Overwrite,
    cancelled: impl Fn() -> bool,
    stages: &impl SaveStages,
) -> Result<ExportRecord, OutputError> {
    write_bytes_with_stages(
        &packet.prepared.snapshot,
        destination,
        overwrite,
        cancelled,
        stages,
        || render_document_pdf(&packet.document),
    )
}

fn write_pdf_with_stages(
    prepared: &PreparedExport,
    destination: Option<&Path>,
    overwrite: Overwrite,
    cancelled: impl Fn() -> bool,
    stages: &impl SaveStages,
) -> Result<ExportRecord, OutputError> {
    // The legacy renderer always emits all three sections. It must never be
    // used to write a reviewed selection with misleading receipt metadata.
    if prepared.snapshot.sections != Some(ReceiptSections::default()) {
        return Err(OutputError::PreparationBlocked);
    }
    write_bytes_with_stages(
        &prepared.snapshot,
        destination,
        overwrite,
        cancelled,
        stages,
        || render_pdf(prepared),
    )
}

fn write_bytes_with_stages(
    snapshot: &ExportSnapshot,
    destination: Option<&Path>,
    overwrite: Overwrite,
    cancelled: impl Fn() -> bool,
    stages: &impl SaveStages,
    render: impl FnOnce() -> Result<Vec<u8>, PdfExportError>,
) -> Result<ExportRecord, OutputError> {
    let path = destination.ok_or(OutputError::Cancelled)?;
    if cancelled() {
        return Err(OutputError::Cancelled);
    }
    if path.symlink_metadata().is_ok() && overwrite != Overwrite::Confirm {
        return Err(OutputError::OverwriteRequired);
    }
    // Reject oversized evidence before touching a destination, rather than
    // writing a PDF whose successful receipt cannot be represented.
    ComparisonBaseline::from_project(snapshot.project()).map_err(OutputError::Verify)?;
    let bytes = render().map_err(OutputError::Pdf)?;
    if cancelled() {
        return Err(OutputError::Cancelled);
    }
    // Recheck immediately before committing, including destinations created
    // while rendering. The UI must request confirmation again in that case.
    if path.symlink_metadata().is_ok() && overwrite != Overwrite::Confirm {
        return Err(OutputError::OverwriteRequired);
    }
    match overwrite {
        Overwrite::Confirm => atomic_write_with_stages(path, &bytes, stages, || {}),
        Overwrite::Decline => atomic_write_new_with_stages(path, &bytes, stages, || {}),
    }
    .map_err(|error| match error {
        SaveError::Io(ref io_error)
            if overwrite == Overwrite::Decline
                && io_error.kind() == io::ErrorKind::AlreadyExists =>
        {
            OutputError::OverwriteRequired
        }
        other => OutputError::Write(other),
    })?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    ExportRecord::verify_completed(snapshot, path, time, &hash).map_err(OutputError::Verify)
}

impl From<io::Error> for OutputError {
    fn from(error: io::Error) -> Self {
        Self::Write(SaveError::Io(error))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportStatus {
    NeverExported,
    Current,
    PacketStale,
    WoodStale,
    /// Historical evidence does not identify a comparable hash algorithm.
    Unknown,
}

impl ExportStatus {
    pub fn for_project(project: &Project) -> Self {
        let Some(last) = project.export_records.last() else {
            let mut normalized = project.clone();
            return if project.validate().is_ok()
                && normalized.assign_missing_stock_aliases().is_ok()
            {
                Self::NeverExported
            } else {
                Self::Unknown
            };
        };
        Self::for_record(project, last)
    }

    /// Compare one historical receipt using the hash version it actually wrote.
    /// Receipt order is not part of manufacturing freshness.
    pub fn for_record(project: &Project, last: &ExportRecord) -> Self {
        // Structural validation permits alias-free legacy input; normalization
        // can still fail when a persisted counter is exhausted.
        if project.validate().is_err() {
            return Self::Unknown;
        }
        let mut normalized = project.clone();
        if normalized.assign_missing_stock_aliases().is_err() {
            return Self::Unknown;
        }
        if last.project_id != project.id {
            return Self::Unknown;
        }
        let version = last.metadata.fingerprint_version.unwrap_or(2);
        if !(2..=CURRENT_FINGERPRINT_VERSION).contains(&version) {
            return Self::Unknown;
        }
        let current = fingerprint_version(&normalized, version);
        // Version-2 hashes did not contain printed stock aliases. Receipts with
        // a recorded mapping can still detect alias changes independently.
        let aliases_changed = version == 2
            && last
                .metadata
                .stock_aliases
                .as_ref()
                .is_some_and(|previous| previous != &normalized.stock_aliases);
        if last.wood_sha256 != current.wood || aliases_changed {
            Self::WoodStale
        } else if last.packet_sha256 != current.packet
            || (version == 3
                && last
                    .metadata
                    .comparison_baseline
                    .as_ref()
                    .is_some_and(|baseline| {
                        normalized.hardware.iter().any(|hardware| {
                            let HardwareKind::Catalog { catalog_id } = hardware.kind else {
                                return false;
                            };
                            let Some(catalog) = normalized
                                .catalog
                                .iter()
                                .find(|catalog| catalog.id == catalog_id)
                            else {
                                return false;
                            };
                            baseline.entries.iter().any(|entry| {
                                entry.kind == ComparisonKind::Catalog
                                    && entry.id == catalog_id
                                    && entry.facts.get("verified")
                                        != Some(&format!("{:?}", catalog.verified_hinge))
                            })
                        })
                    }))
        {
            Self::PacketStale
        } else if last.metadata.fingerprint_version.is_none()
            && last.metadata.stock_aliases.is_none()
        {
            // An unversioned historical receipt has no printed-alias evidence.
            // Its legacy hash is comparable only while aliases still match the
            // deterministic migration of its unchanged stock/order/source.
            let mut migrated = project.clone();
            migrated.stock_aliases.clear();
            migrated.next_stock_s_alias = 1;
            migrated.next_stock_o_alias = 1;
            if migrated.assign_missing_stock_aliases().is_err() {
                return Self::Unknown;
            }
            if normalized.stock_aliases != migrated.stock_aliases {
                Self::Unknown
            } else {
                Self::Current
            }
        } else {
            Self::Current
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::NeverExported => "export-never",
            Self::Current => "export-current",
            Self::PacketStale | Self::WoodStale => "export-stale",
            Self::Unknown => "export-unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::ProjectEditor;
    use crate::domain::{
        Allocation, Board, BoardGrain, CatalogReference, Hardware, Material, Stock, StockGrain,
        StockSource,
    };
    use crate::money::{Currency, Money};
    use crate::persistence::{prepare_reader, save};
    use crate::units::{Length, Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn fixture() -> Project {
        let mut p = Project::new("Cabinet", Currency::Brl);
        let pose = Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap();
        let material_id = Uuid::new_v4();
        let stock_id = Uuid::new_v4();
        let catalog_id = Uuid::new_v4();
        p.materials.push(Material {
            id: material_id,
            name: "Ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        });
        p.stock.push(Stock {
            id: stock_id,
            name: "Sheet".into(),
            material_id,
            length: mm(300),
            width: mm(200),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::ToPurchase,
            price: Some(Money::new(Currency::Brl, 100).unwrap()),
            priority: 0,
            trim: [Length::ZERO; 4],
        });
        for _ in 0..2 {
            let id = Uuid::new_v4();
            p.boards.push(Board {
                id,
                name: "Shelf".into(),
                material_id,
                length: mm(100),
                width: mm(50),
                thickness: mm(18),
                grain_override: None,
                parent_id: None,
                pose,
            });
            p.allocations.push(Allocation {
                id: Uuid::new_v4(),
                board_id: id,
                stock_id,
                origin: [Length::ZERO; 2],
                quarter_turn: false,
                locked: false,
            });
        }
        p.catalog.push(CatalogReference {
            id: catalog_id,
            name: "Pinned".into(),
            product_id: "H1".into(),
            plate_id: None,
            source: "manufacturer".into(),
            revision: "1".into(),
            installation_dimensions: std::collections::HashMap::from([("cup".into(), mm(11))]),
            verified_hinge: None,
            origin: None,
        });
        p.hardware.push(Hardware {
            id: Uuid::new_v4(),
            name: "Hinge".into(),
            parent_id: None,
            pose,
            kind: HardwareKind::Catalog { catalog_id },
        });
        p
    }

    fn settings() -> ExportSettings {
        ExportSettings {
            language: Language::PtBr,
            units: Unit::Mm,
        }
    }

    #[test]
    fn persisted_v3_hash_uses_its_original_projection() {
        let mut project = fixture();
        project.catalog[0] = crate::hardware_catalog::builtin_hinge();
        project.hardware[0].kind = HardwareKind::Catalog {
            catalog_id: project.catalog[0].id,
        };
        project.assign_missing_stock_aliases().unwrap();
        project.validate().unwrap();
        let old = fingerprint_version(&project, 3);
        project.export_records.push(ExportRecord {
            project_id: project.id,
            revision: project.revision,
            wood_sha256: old.wood.clone(),
            packet_sha256: old.packet.clone(),
            settings: settings(),
            path: PathBuf::from("older.pdf"),
            completed_unix_ms: 1,
            file_sha256: "0".repeat(64),
            metadata: Box::new(ReceiptMetadata {
                metadata_version: Some(1),
                mode: Some(ExportMode::Draft),
                sections: Some(ReceiptSections::default()),
                fingerprint_version: Some(3),
                layout_version: Some(1),
                stock_aliases: Some(project.stock_aliases.clone()),
                comparison_baseline: Some(ComparisonBaseline::from_project(&project).unwrap()),
            }),
        });
        assert_eq!(ExportStatus::for_project(&project), ExportStatus::Current);
        let new_packet = fingerprint(&project).packet;
        project.catalog[0].verified_hinge = None;
        assert_eq!(fingerprint_version(&project, 3), old);
        assert_eq!(
            ExportStatus::for_project(&project),
            ExportStatus::PacketStale
        );
        assert_ne!(fingerprint(&project).packet, new_packet);
    }

    fn cutting_fixture() -> Project {
        let mut p = fixture();
        p.boards.truncate(1);
        p.allocations.truncate(1);
        p.stock[0].length = mm(100);
        p.stock[0].width = mm(50);
        p.confirmed_shop_kerf = Some(p.cutting_kerf);
        p
    }

    fn ready(p: &Project, budget: usize) -> Result<PreparedExport, ShopReadyBlocked> {
        prepare_export_with_budget(p, settings(), ExportMode::ShopReady, budget)
    }

    fn hinged_cutting_fixture() -> Project {
        use crate::domain::{BoardEdge, BoardFace, HingeInstallation, HingeMountingSide};
        let mut p = cutting_fixture();
        p.hardware.clear();
        p.catalog.clear();
        let catalog = crate::hardware_catalog::builtin_hinge();
        let mut mount = p.boards[0].duplicate();
        mount.name = "Mount".into();
        p.stock.push(Stock {
            id: Uuid::new_v4(),
            name: "Mount sheet".into(),
            ..p.stock[0].clone()
        });
        p.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: mount.id,
            stock_id: p.stock[1].id,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: false,
        });
        p.boards.push(mount);
        let hinge = HingeInstallation {
            id: Uuid::new_v4(),
            door_board_id: p.boards[0].id,
            mounting_board_id: p.boards[1].id,
            catalog_id: catalog.id,
            side: HingeMountingSide {
                door_edge: BoardEdge::MinX,
                door_face: BoardFace::MinZ,
                mount_front_edge: BoardEdge::MinX,
                mount_face: BoardFace::MaxZ,
            },
            door_y: mm(25),
            mount_y: mm(25),
            cup_edge_setback: mm(3),
            overlay: mm(15),
            inset_depth: Default::default(),
        };
        // The 35 mm cup and 32 mm plate pitch fit in 50 mm board widths.
        p.catalog.push(catalog);
        p.hinge_installations.push(hinge.clone());
        let proposal = crate::door_joint::preview(
            &p,
            Uuid::new_v4(),
            hinge.door_board_id,
            hinge.mounting_board_id,
            vec![hinge.id],
        )
        .unwrap();
        p.door_joints.push(proposal.joint);
        p
    }

    #[test]
    fn validated_hinge_guidance_and_independent_packet_staleness_survive_roundtrip() {
        let mut p = hinged_cutting_fixture();
        let plan = ready(&p, 100).unwrap();
        assert!(plan.wood_issues.is_empty());
        assert_eq!(plan.installation_guidance.len(), 1);
        let g = &plan.installation_guidance[0];
        assert_eq!(g.references.product_id, crate::hardware_catalog::KIT_ID);
        assert_eq!(
            g.references.plate_id.as_deref(),
            Some(crate::hardware_catalog::PLATE_ID)
        );
        assert_eq!(g.references.printed_page, 23);
        assert_eq!(g.references.cup_center_um, [20_500, 25_000, 0]);
        assert_eq!(
            g.references.plate_hole_centers_um,
            [[37_000, 9_000, 18_000], [37_000, 41_000, 18_000]]
        );
        let baseline = fingerprint(&p);
        let historical = fingerprint_version(&p, 2);
        p.export_records.push(ExportRecord {
            project_id: p.id,
            revision: p.revision,
            wood_sha256: historical.wood,
            packet_sha256: historical.packet,
            settings: settings(),
            path: PathBuf::from("previous-packet.pdf"),
            completed_unix_ms: 1,
            file_sha256: "0".repeat(64),
            metadata: Box::default(),
        });
        assert_eq!(ExportStatus::for_project(&p), ExportStatus::Current);
        p.hardware.push(Hardware {
            id: Uuid::new_v4(),
            name: "Foot".into(),
            parent_id: None,
            pose: p.boards[0].pose,
            kind: HardwareKind::Placeholder {
                dimensions: [mm(30), mm(30), mm(80)],
            },
        });
        assert_eq!(fingerprint(&p).wood, baseline.wood);
        assert_ne!(fingerprint(&p).packet, baseline.packet);
        assert_eq!(ExportStatus::for_project(&p), ExportStatus::PacketStale);
        let with_foot = fingerprint(&p);
        p.catalog[0].product_id = "other kit".into();
        p.catalog[0].verified_hinge = None;
        assert_eq!(fingerprint(&p).wood, with_foot.wood);
        assert_ne!(fingerprint(&p).packet, with_foot.packet);
        assert_eq!(ExportStatus::for_project(&p), ExportStatus::PacketStale);
        assert!(ready(&p, 100).unwrap().installation_guidance.is_empty());
        let reopened = prepare_reader(serde_json::to_vec(&p).unwrap().as_slice()).unwrap();
        assert_eq!(
            reopened.project().hinge_installations,
            p.hinge_installations
        );
        assert_eq!(reopened.project().door_joints, p.door_joints);
        assert_eq!(reopened.project().hardware, p.hardware);
    }

    #[test]
    fn invalid_installations_and_joint_review_withhold_numbers_without_blocking_wood() {
        let p = hinged_cutting_fixture();
        let mut outside = p.clone();
        outside.hinge_installations[0].door_y = mm(49);
        let plan = ready(&outside, 100).unwrap();
        assert!(plan.wood_issues.is_empty());
        assert!(plan.installation_guidance.is_empty());
        assert!(
            plan.withheld_installation_guidance
                .contains(&outside.hinge_installations[0].id)
        );
        assert!(plan.notices.iter().any(|n| matches!(
            n,
            ExportIssue::Installation {
                reason: InstallationIssue::CupOutsideDoor,
                ..
            }
        )));
        assert_eq!(fingerprint(&outside).wood, fingerprint(&p).wood);
        let mut drift = p.clone();
        drift.boards[0].pose.translation_mm[0] += 1.0;
        assert_eq!(fingerprint(&drift).wood, fingerprint(&p).wood);
        assert_ne!(fingerprint(&drift).packet, fingerprint(&p).packet);
        assert!(ready(&drift, 100).unwrap().installation_guidance.is_empty());
        assert!(
            ready(&drift, 100)
                .unwrap()
                .notices
                .iter()
                .any(|n| matches!(n, ExportIssue::JointNeedsReview { .. }))
        );
        let mut thin = p.clone();
        thin.boards[0].thickness = mm(14);
        let plan = ready(&thin, 100).unwrap_err(); // wood allocation thickness mismatch remains its own gate
        assert!(!plan.issues.is_empty());
        // An independently allocated thin door is still wood-shop-ready.
        thin.stock[0].thickness = mm(14);
        let plan = ready(&thin, 100).unwrap();
        assert!(plan.installation_guidance.is_empty());
        assert!(plan.notices.iter().any(|n| matches!(
            n,
            ExportIssue::Installation {
                reason: InstallationIssue::UnsupportedThickness,
                ..
            }
        )));
    }

    #[test]
    fn bilingual_hardware_pdf_prints_verified_source_and_omits_invalid_coordinates() {
        let project = hinged_cutting_fixture();
        for (language, warning, heading) in [
            (Language::En, "approximate fixed-axis", "Cup centre"),
            (
                Language::PtBr,
                "aproximada de eixo fixo",
                "Centro do caneco",
            ),
        ] {
            let settings = ExportSettings {
                language,
                units: Unit::Mm,
            };
            let valid = prepare_export(&project, settings, ExportMode::ShopReady).unwrap();
            let mut invalid_project = project.clone();
            invalid_project.hinge_installations[0].door_y = mm(49);
            let invalid =
                prepare_export(&invalid_project, settings, ExportMode::ShopReady).unwrap();
            assert!(invalid.wood_issues.is_empty());
            let path = std::env::temp_dir().join(format!("pmcab-hinge-{}.pdf", Uuid::new_v4()));
            let extract = |plan: &PreparedExport| -> Option<String> {
                fs::write(&path, render_pdf(plan).unwrap()).unwrap();
                let output = std::process::Command::new("pdftotext")
                    .args(["-layout", path.to_str().unwrap(), "-"])
                    .output()
                    .ok()?;
                assert!(output.status.success());
                String::from_utf8(output.stdout).ok()
            };
            if let Some(text) = extract(&valid) {
                assert!(text.contains(crate::hardware_catalog::KIT_ID));
                assert!(text.contains(crate::hardware_catalog::PLATE_ID));
                assert!(text.contains("23"));
                assert!(text.contains("11.3 mm") || text.contains("11,3 mm"));
                assert!(text.contains("37 mm"));
                assert!(text.contains(heading));
                assert!(text.contains(warning));
            }
            if let Some(text) = extract(&invalid) {
                assert!(!text.contains(heading));
                assert!(!text.contains("20.5 mm") && !text.contains("20,5 mm"));
                assert!(text.contains(warning));
            }
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn shop_gate_hidden_unallocated_conflicted_and_valid() {
        let mut p = cutting_fixture();
        assert_eq!(ready(&p, 100).unwrap().witnesses.len(), 1);
        let mut hidden = p.boards[0].duplicate();
        hidden.name = "Hidden shelf".into();
        // Visibility is view state: every board is manufacturing demand.
        p.boards.push(hidden.clone());
        let blocked = ready(&p, 100).unwrap_err();
        assert!(blocked.issues.iter().any(|issue| matches!(issue, ExportIssue::Board { id, reasons, .. } if *id == hidden.id && reasons.contains(&Reason::MissingAllocation))));
        let draft = prepare_export(&p, settings(), ExportMode::Draft).unwrap();
        assert_eq!(draft.preview_watermark, Some("DRAFT / NOT FOR CUTTING"));
        assert!(!draft.wood_issues.is_empty());
        p.boards.pop();
        p.allocations[0].origin[0] = mm(2);
        assert!(ready(&p, 100).is_err());
        p.allocations[0].origin[0] = Length::ZERO;
        assert!(ready(&p, 100).is_ok());
    }

    #[test]
    fn unknown_proof_price_hardware_and_kerf_confirmation() {
        let mut p = cutting_fixture();
        assert!(
            ready(&p, 0)
                .unwrap_err()
                .issues
                .iter()
                .any(|issue| matches!(
                    issue,
                    ExportIssue::Sheet {
                        reason: SheetIssue::BudgetExhausted,
                        ..
                    }
                ))
        );
        p.stock[0].price = None;
        let plan = ready(&p, 100).unwrap();
        assert!(
            plan.notices
                .iter()
                .any(|issue| matches!(issue, ExportIssue::UnknownPrice { stock_id: Some(_) }))
        );
        p.hardware[0].kind = HardwareKind::Catalog {
            catalog_id: Uuid::new_v4(),
        };
        let plan = ready(&p, 100).unwrap();
        assert!(plan.notices.iter().any(|issue| matches!(
            issue,
            ExportIssue::Hardware {
                reason: HardwareIssue::MissingCatalog(_),
                ..
            }
        )));
        assert_eq!(plan.withheld_installation_guidance, vec![p.hardware[0].id]);
        p.confirmed_shop_kerf = None;
        assert!(
            ready(&p, 100)
                .unwrap_err()
                .issues
                .iter()
                .any(|issue| matches!(issue, ExportIssue::KerfUnconfirmed(_)))
        );
        let mut editor = ProjectEditor::new(cutting_fixture()).unwrap();
        editor.set_cutting_kerf(mm(6)).unwrap();
        assert_eq!(editor.project().confirmed_shop_kerf, None);
        editor.confirm_shop_kerf().unwrap();
        assert_eq!(editor.project().confirmed_shop_kerf, Some(mm(6)));
        let reopened: Project =
            serde_json::from_slice(&serde_json::to_vec(editor.project()).unwrap()).unwrap();
        assert_eq!(reopened.confirmed_shop_kerf, Some(mm(6)));
        assert!(ready(&reopened, 100).is_ok());
        let mut legacy = serde_json::to_value(&reopened).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("confirmed_shop_kerf");
        let legacy: Project = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.confirmed_shop_kerf, None);
        assert!(ready(&legacy, 100).is_err());
    }

    #[test]
    fn canonical_order_and_change_matrix() {
        let p = fixture();
        let original = fingerprint(&p);
        let mut reordered = p.clone();
        reordered.boards.reverse();
        reordered.allocations.reverse();
        reordered.catalog[0]
            .installation_dimensions
            .insert("edge".into(), mm(3));
        reordered.catalog[0]
            .installation_dimensions
            .insert("depth".into(), mm(4));
        let with_dimensions = fingerprint(&reordered);
        reordered.catalog[0].installation_dimensions = std::collections::HashMap::from([
            ("depth".into(), mm(4)),
            ("cup".into(), mm(11)),
            ("edge".into(), mm(3)),
        ]);
        assert_eq!(fingerprint(&reordered), with_dimensions);
        assert_eq!(original.wood, with_dimensions.wood);
        reordered.catalog[0].installation_dimensions.remove("depth");
        reordered.catalog[0].installation_dimensions.remove("edge");
        assert_eq!(fingerprint(&reordered), original);

        for change in [
            |q: &mut Project| q.boards[0].length = mm(101),
            |q: &mut Project| q.cutting_kerf = mm(3),
            |q: &mut Project| q.stock[0].price = None,
            |q: &mut Project| q.boards[0].name = "Renamed".into(),
            |q: &mut Project| q.allocations[0].origin[0] = mm(1),
            |q: &mut Project| q.materials[0].name = "Birch".into(),
        ] {
            let mut q = p.clone();
            change(&mut q);
            assert_ne!(fingerprint(&q).wood, original.wood);
        }
        let mut q = p.clone();
        q.hardware[0].name = "Other".into();
        assert_eq!(fingerprint(&q).wood, original.wood);
        assert_ne!(fingerprint(&q).packet, original.packet);
        let mut q = p.clone();
        q.catalog[0].revision = "2".into();
        assert_ne!(fingerprint(&q).packet, original.packet);
        let mut q = p.clone();
        q.hardware.clear();
        assert_ne!(fingerprint(&q).packet, original.packet);
        let mut q = p.clone();
        q.boards[0].pose.translation_mm[0] = 50.0;
        q.hardware[0].pose.translation_mm[0] = 40.0;
        q.grid_spacing = mm(20);
        q.display_unit = Unit::Inch;
        q.allocations[0].locked = true;
        q.revision += 1;
        assert_eq!(fingerprint(&q), original);
    }

    #[test]
    fn completed_receipts_survive_roundtrip_undo_and_previews_do_not_record() {
        let mut editor = ProjectEditor::new(fixture()).unwrap();
        let snapshot = editor.export_snapshot(settings()).unwrap();
        let path = std::env::temp_dir().join(format!("pmcab-export-{}.pdf", Uuid::new_v4()));
        assert!(matches!(
            editor.record_completed_export(&snapshot, &path, 12, &"0".repeat(64)),
            Err(ExportError::Io(_))
        ));
        assert_eq!(editor.last_export_status(), ExportStatus::NeverExported);
        fs::write(&path, b"finished output").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"finished output"));
        assert!(matches!(
            editor.record_completed_export(&snapshot, &path, 12, &"0".repeat(64)),
            Err(ExportError::FileMismatch)
        ));
        editor
            .record_completed_export(&snapshot, &path, 12, &hash)
            .unwrap();
        assert_eq!(editor.last_export_status(), ExportStatus::Current);
        editor.begin_preview();
        editor
            .update_preview(|p| -> Result<(), ()> {
                p.boards[0].width = mm(200);
                Ok(())
            })
            .unwrap();
        assert_eq!(editor.last_export_status(), ExportStatus::Current);
        editor.cancel_preview();
        editor
            .transact(|p| -> Result<(), ()> {
                p.hardware[0].name = "New hinge".into();
                Ok(())
            })
            .unwrap();
        assert_eq!(editor.last_export_status(), ExportStatus::PacketStale);
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].name = "New shelf".into();
                Ok(())
            })
            .unwrap();
        assert_eq!(editor.last_export_status(), ExportStatus::WoodStale);
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.last_export_status(), ExportStatus::Current);
        let project_path = path.with_extension("pmcab");
        save(&mut editor, &project_path).unwrap();
        let reopened = prepare_reader(fs::File::open(&project_path).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(reopened.last_export_status(), ExportStatus::Current);
        assert_eq!(reopened.project().export_records[0].path, path);
        assert_eq!(reopened.project().export_records[0].file_sha256, hash);
        fs::remove_file(path).unwrap();
        fs::remove_file(project_path).unwrap();
    }

    #[test]
    fn pdf_pipeline_cancel_decline_failures_receipt_roundtrip_and_stale_snapshot() {
        use crate::persistence::SaveStage;
        use std::io::{self, Write};
        struct Fail(SaveStage);
        impl SaveStages for Fail {
            fn before(&self, stage: SaveStage) -> io::Result<()> {
                if stage == self.0 && stage != SaveStage::Write {
                    Err(io::Error::other("injected"))
                } else {
                    Ok(())
                }
            }
            fn write(&self, file: &mut fs::File, bytes: &[u8]) -> io::Result<()> {
                if self.0 == SaveStage::Write {
                    file.write_all(&bytes[..bytes.len() / 2])?;
                    Err(io::Error::other("partial write"))
                } else {
                    file.write_all(bytes)
                }
            }
        }
        let dir = std::env::temp_dir().join(format!("pmcab-pdf-{}", Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("packet.pdf");
        fs::write(&path, b"previous PDF").unwrap();
        let mut editor = ProjectEditor::new(cutting_fixture()).unwrap();
        let original = editor.project().clone();
        let settings = ExportSettings {
            language: Language::PtBr,
            units: Unit::Foot,
        };
        let prepared = prepare_export(editor.project(), settings, ExportMode::ShopReady).unwrap();
        let fresh = dir.join("fresh.pdf");
        let first = write_pdf(&prepared, Some(&fresh), Overwrite::Decline).unwrap();
        assert!(fs::read(&fresh).unwrap().starts_with(b"%PDF-"));
        assert_eq!(first.path, fresh);
        fs::remove_file(&fresh).unwrap();
        // Exercise the atomic create-if-absent step after the preflight check.
        struct Race(PathBuf);
        impl SaveStages for Race {
            fn before(&self, stage: SaveStage) -> io::Result<()> {
                if stage == SaveStage::Rename {
                    fs::write(&self.0, b"racing file")?;
                }
                Ok(())
            }
        }
        assert!(matches!(
            write_pdf_with_stages(
                &prepared,
                Some(&fresh),
                Overwrite::Decline,
                || false,
                &Race(fresh.clone())
            ),
            Err(OutputError::OverwriteRequired)
        ));
        assert_eq!(fs::read(&fresh).unwrap(), b"racing file");
        fs::remove_file(&fresh).unwrap();
        assert!(matches!(
            write_pdf(&prepared, None, Overwrite::Confirm),
            Err(OutputError::Cancelled)
        ));
        assert!(matches!(
            write_pdf(&prepared, Some(&path), Overwrite::Decline),
            Err(OutputError::OverwriteRequired)
        ));
        assert!(matches!(
            write_pdf_cancellable(&prepared, Some(&path), Overwrite::Confirm, || true),
            Err(OutputError::Cancelled)
        ));
        for stage in [
            SaveStage::Create,
            SaveStage::Write,
            SaveStage::Flush,
            SaveStage::SyncFile,
            SaveStage::Rename,
        ] {
            assert!(
                matches!(
                    write_pdf_with_stages(
                        &prepared,
                        Some(&path),
                        Overwrite::Confirm,
                        || false,
                        &Fail(stage)
                    ),
                    Err(OutputError::Write(SaveError::Io(_)))
                ),
                "{stage:?}"
            );
            assert_eq!(fs::read(&path).unwrap(), b"previous PDF");
            assert_eq!(editor.project(), &original);
            assert_eq!(editor.last_export_status(), ExportStatus::NeverExported);
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        }
        let uncertain = write_pdf_with_stages(
            &prepared,
            Some(&path),
            Overwrite::Confirm,
            || false,
            &Fail(SaveStage::SyncDirectory),
        );
        assert!(matches!(
            uncertain,
            Err(OutputError::Write(SaveError::CommittedDurabilityUncertain(
                _
            )))
        ));
        assert!(fs::read(&path).unwrap().starts_with(b"%PDF-"));
        assert_eq!(editor.last_export_status(), ExportStatus::NeverExported);
        // A newer edit cannot turn the old worker result into a current plan.
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].name = "Later shelf".into();
                Ok(())
            })
            .unwrap();
        let receipt = write_pdf(&prepared, Some(&path), Overwrite::Confirm).unwrap();
        assert_eq!(receipt.settings, settings);
        editor
            .record_completed_export(
                &prepared.snapshot,
                &path,
                receipt.completed_unix_ms,
                &receipt.file_sha256,
            )
            .unwrap();
        assert_eq!(editor.last_export_status(), ExportStatus::WoodStale);
        let project_path = dir.join("project.pmcab");
        save(&mut editor, &project_path).unwrap();
        let reopened = prepare_reader(fs::File::open(&project_path).unwrap())
            .unwrap()
            .into_editor();
        assert_eq!(reopened.project().export_records.last(), Some(&receipt));
        assert_eq!(reopened.last_export_status(), ExportStatus::WoodStale);
        fs::remove_dir_all(dir).unwrap();
    }
}
