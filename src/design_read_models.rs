//! Read-only Design presentation. Feed it an already-built stock snapshot on
//! project changes; selection, expansion and visibility can be reprojected
//! without running cutting witnesses again.
use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::allocation_diagnostics::{BoardDiagnostic, Status};
use crate::domain::{BoardGrain, DomainError, HardwareKind, Project, SrgbColor, StockSource};
use crate::measurements::{self, Frame, Measurement, MeasurementError, Scope};
use crate::stock_read_models::StockReadModel;
use crate::units::{Length, Pose};

/// Borrowed scene/session state; the portable project owns none of these flags.
pub struct DesignView<'a> {
    pub selected: &'a HashSet<Uuid>,
    pub active: Option<Uuid>,
    pub hidden: &'a HashSet<Uuid>,
    pub expanded: &'a HashSet<Uuid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Assembly,
    Board,
    Hardware,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlinerRow {
    pub id: Uuid,
    pub name: String,
    pub kind: ObjectKind,
    pub parent_id: Option<Uuid>,
    pub depth: usize,
    pub selected: bool,
    pub active: bool,
    pub hidden_directly: bool,
    pub visible: bool,
    /// Only meaningful for assemblies; collapse does not remove the row.
    pub expanded: Option<bool>,
    /// Number of descendant boards with issues, or one for an affected board.
    pub issue_count: usize,
    pub allocation: Option<BoardDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignMaterialRow {
    pub id: Uuid,
    pub name: String,
    pub color: SrgbColor,
    pub default_thickness: Length,
    pub board_count: usize,
    pub stock_piece_count: usize,
    pub unallocated_board_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignStockRow {
    pub id: Uuid,
    pub alias: String,
    pub global_rank: usize,
    pub name: String,
    pub material_id: Uuid,
    pub size: [Length; 3],
    pub source: StockSource,
    pub part_count: usize,
}

/// This describes an equality with the *current* material, not the historical
/// operation that produced the board's independently stored thickness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThicknessProvenance {
    MatchesCurrentMaterialDefault,
    StoredBoardValueDiffers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrainProvenance {
    MaterialDefault,
    BoardOverride,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectFrame {
    pub local_to: Option<Uuid>, // None is world; Some(id) is the parent assembly.
    pub local_pose: Pose,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScopedBounds {
    pub body: Result<Measurement, MeasurementError>,
    pub overall: Result<Measurement, MeasurementError>,
}

impl ScopedBounds {
    /// Caller chooses World or the selected object's frame, never an implicit
    /// axis-aligned board dimension masquerading as a bounding measurement.
    pub fn measure(project: &Project, ids: &[Uuid], frame: Frame) -> Self {
        Self {
            body: measurements::measure(project, ids, Scope::Body, frame),
            overall: measurements::measure(project, ids, Scope::Overall, frame),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AllocationReference {
    pub allocation_id: Uuid,
    pub stock_id: Uuid,
    /// None means the supplied cached snapshot does not contain this stock ID;
    /// a validated project itself cannot have a dangling allocation reference.
    pub stock_alias: Option<String>,
    pub origin: [Length; 2],
    pub quarter_turn: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardInspector {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub parent_name: Option<String>,
    pub visible: bool,
    pub material_id: Uuid,
    pub material_name: String,
    pub material_color: SrgbColor,
    pub dimensions: [Length; 3],
    pub material_default_thickness: Length,
    pub thickness_provenance: ThicknessProvenance,
    pub grain: BoardGrain,
    pub grain_provenance: GrainProvenance,
    pub frame: ObjectFrame,
    pub allocation: BoardDiagnostic,
    pub stock: Vec<AllocationReference>,
    pub bounds_world: ScopedBounds,
    pub bounds_object: ScopedBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssemblyInspector {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub parent_name: Option<String>,
    pub visible: bool,
    pub frame: ObjectFrame,
    pub descendant_board_count: usize,
    pub issue_count: usize,
    pub bounds_world: ScopedBounds,
    pub bounds_object: ScopedBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HardwareInspector {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub visible: bool,
    pub frame: ObjectFrame,
    pub dimensions: Option<[Length; 3]>,
    pub bounds_world: ScopedBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MultiInspector {
    pub ids: Vec<Uuid>,
    pub active: Option<Uuid>,
    pub board_count: usize,
    pub assembly_count: usize,
    pub hardware_count: usize,
    pub bounds_world: ScopedBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DesignInspector {
    None,
    Board(BoardInspector),
    Assembly(AssemblyInspector),
    Hardware(HardwareInspector),
    Multi(MultiInspector),
}

pub struct DesignReadModel {
    /// All rows in parent-before-child order, even under collapsed/hidden parents.
    /// UI filters descendants by `expanded`, but may always reveal/search by ID.
    pub outliner: Vec<OutlinerRow>,
    pub materials: Vec<DesignMaterialRow>,
    pub stock: Vec<DesignStockRow>,
    pub inspector: DesignInspector,
}

impl DesignReadModel {
    /// `stock` must be the cached snapshot of this same project/revision. Reuse
    /// it across view-only changes; build it on manufacturing/pricing edits.
    pub fn build(
        project: &Project,
        stock: &StockReadModel,
        view: DesignView<'_>,
    ) -> Result<Self, DomainError> {
        project.validate()?;
        let diagnostics: HashMap<_, _> = stock.boards.iter().map(|d| (d.board_id, d)).collect();
        let mut rows = Vec::new();
        let mut children: HashMap<Option<Uuid>, Vec<(Uuid, ObjectKind, &str)>> = HashMap::new();
        for a in &project.assemblies {
            children
                .entry(a.parent_id)
                .or_default()
                .push((a.id, ObjectKind::Assembly, &a.name));
        }
        for b in &project.boards {
            children
                .entry(b.parent_id)
                .or_default()
                .push((b.id, ObjectKind::Board, &b.name));
        }
        for h in &project.hardware {
            children
                .entry(h.parent_id)
                .or_default()
                .push((h.id, ObjectKind::Hardware, &h.name));
        }
        fn visit(
            parent: Option<Uuid>,
            depth: usize,
            inherited_visible: bool,
            children: &HashMap<Option<Uuid>, Vec<(Uuid, ObjectKind, &str)>>,
            diagnostics: &HashMap<Uuid, &BoardDiagnostic>,
            view: &DesignView<'_>,
            rows: &mut Vec<OutlinerRow>,
        ) -> usize {
            let mut issues = 0;
            for &(id, kind, name) in children.get(&parent).into_iter().flatten() {
                let visible = inherited_visible && !view.hidden.contains(&id);
                let index = rows.len();
                let diagnostic = diagnostics.get(&id).copied();
                rows.push(OutlinerRow {
                    id,
                    name: name.into(),
                    kind,
                    parent_id: parent,
                    depth,
                    selected: view.selected.contains(&id),
                    active: view.active == Some(id),
                    hidden_directly: view.hidden.contains(&id),
                    visible,
                    expanded: (kind == ObjectKind::Assembly).then(|| view.expanded.contains(&id)),
                    issue_count: 0,
                    allocation: diagnostic.cloned(),
                });
                let own =
                    usize::from(diagnostic.is_some_and(|d| d.status != Status::AllocatedValid));
                let descendants = if kind == ObjectKind::Assembly {
                    visit(
                        Some(id),
                        depth + 1,
                        visible,
                        children,
                        diagnostics,
                        view,
                        rows,
                    )
                } else {
                    0
                };
                rows[index].issue_count = own + descendants;
                issues += own + descendants;
            }
            issues
        }
        visit(None, 0, true, &children, &diagnostics, &view, &mut rows);
        let materials = stock
            .materials
            .iter()
            .map(|m| DesignMaterialRow {
                id: m.id,
                name: m.name.clone(),
                color: project.material_color(m.id),
                default_thickness: m.default_thickness,
                board_count: m.board_count,
                stock_piece_count: m.stock_piece_count,
                unallocated_board_count: m.unallocated_board_count,
            })
            .collect();
        let stock_rows = stock
            .pieces
            .iter()
            .map(|s| DesignStockRow {
                id: s.id,
                alias: s.alias.clone(),
                global_rank: s.global_rank,
                name: s.name.clone(),
                material_id: s.material_id,
                size: [s.length, s.width, s.measured_thickness],
                source: s.source,
                part_count: s.parts.len(),
            })
            .collect();
        let mut ids: Vec<_> = view
            .selected
            .iter()
            .copied()
            .filter(|id| rows.iter().any(|r| r.id == *id))
            .collect();
        ids.sort_unstable();
        let inspector = match ids.as_slice() {
            [] => DesignInspector::None,
            [id] => {
                let id = *id;
                let row = rows.iter().find(|r| r.id == id).expect("selected row");
                let parent_name = |parent: Option<Uuid>| {
                    parent.and_then(|p| {
                        project
                            .assemblies
                            .iter()
                            .find(|a| a.id == p)
                            .map(|a| a.name.clone())
                    })
                };
                if let Some(b) = project.boards.iter().find(|b| b.id == id) {
                    let material = project
                        .materials
                        .iter()
                        .find(|m| m.id == b.material_id)
                        .expect("validated material");
                    DesignInspector::Board(BoardInspector {
                        id,
                        name: b.name.clone(),
                        parent_id: b.parent_id,
                        parent_name: parent_name(b.parent_id),
                        visible: row.visible,
                        material_id: material.id,
                        material_name: material.name.clone(),
                        material_color: project.material_color(material.id),
                        dimensions: b.blank_dimensions(),
                        material_default_thickness: material.default_thickness,
                        thickness_provenance: if b.thickness == material.default_thickness {
                            ThicknessProvenance::MatchesCurrentMaterialDefault
                        } else {
                            ThicknessProvenance::StoredBoardValueDiffers
                        },
                        grain: b.effective_grain(material),
                        grain_provenance: if b.grain_override.is_some() {
                            GrainProvenance::BoardOverride
                        } else {
                            GrainProvenance::MaterialDefault
                        },
                        frame: ObjectFrame {
                            local_to: b.parent_id,
                            local_pose: b.pose,
                        },
                        allocation: diagnostics[&id].clone(),
                        stock: project
                            .allocations
                            .iter()
                            .filter(|a| a.board_id == id)
                            .map(|a| AllocationReference {
                                allocation_id: a.id,
                                stock_id: a.stock_id,
                                stock_alias: stock.miniature(a.stock_id).map(|s| s.alias.clone()),
                                origin: a.origin,
                                quarter_turn: a.quarter_turn,
                                locked: a.locked,
                            })
                            .collect(),
                        bounds_world: ScopedBounds::measure(project, &[id], Frame::World),
                        bounds_object: ScopedBounds::measure(project, &[id], Frame::Object(id)),
                    })
                } else if let Some(a) = project.assemblies.iter().find(|a| a.id == id) {
                    let descendants = project
                        .boards
                        .iter()
                        .filter(|b| {
                            let mut parent = b.parent_id;
                            while let Some(p) = parent {
                                if p == id {
                                    return true;
                                }
                                parent = project
                                    .assemblies
                                    .iter()
                                    .find(|a| a.id == p)
                                    .and_then(|a| a.parent_id);
                            }
                            false
                        })
                        .count();
                    DesignInspector::Assembly(AssemblyInspector {
                        id,
                        name: a.name.clone(),
                        parent_id: a.parent_id,
                        parent_name: parent_name(a.parent_id),
                        visible: row.visible,
                        frame: ObjectFrame {
                            local_to: a.parent_id,
                            local_pose: a.pose,
                        },
                        descendant_board_count: descendants,
                        issue_count: row.issue_count,
                        bounds_world: ScopedBounds::measure(project, &[id], Frame::World),
                        bounds_object: ScopedBounds::measure(project, &[id], Frame::Object(id)),
                    })
                } else {
                    let h = project
                        .hardware
                        .iter()
                        .find(|h| h.id == id)
                        .expect("selected hardware");
                    DesignInspector::Hardware(HardwareInspector {
                        id,
                        name: h.name.clone(),
                        parent_id: h.parent_id,
                        visible: row.visible,
                        frame: ObjectFrame {
                            local_to: h.parent_id,
                            local_pose: h.pose,
                        },
                        dimensions: match h.kind {
                            HardwareKind::Placeholder { dimensions } => Some(dimensions),
                            HardwareKind::Catalog { .. } => None,
                        },
                        bounds_world: ScopedBounds::measure(project, &[id], Frame::World),
                    })
                }
            }
            _ => DesignInspector::Multi(MultiInspector {
                board_count: ids
                    .iter()
                    .filter(|id| project.boards.iter().any(|b| b.id == **id))
                    .count(),
                assembly_count: ids
                    .iter()
                    .filter(|id| project.assemblies.iter().any(|a| a.id == **id))
                    .count(),
                hardware_count: ids
                    .iter()
                    .filter(|id| project.hardware.iter().any(|h| h.id == **id))
                    .count(),
                active: view.active.filter(|id| ids.contains(id)),
                bounds_world: ScopedBounds::measure(project, &ids, Frame::World),
                ids,
            }),
        };
        Ok(Self {
            outliner: rows,
            materials,
            stock: stock_rows,
            inspector,
        })
    }
}
