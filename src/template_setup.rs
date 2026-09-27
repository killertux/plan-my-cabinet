//! Staged, headless Welcome template setup. Nothing here replaces a document:
//! the desktop host resolves edit/save/replacement guards before `generate`.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{BoardGrain, DomainError, Material, Project, SrgbColor};
use crate::first_fit::{FirstFit, allocate_new_board};
use crate::money::Currency;
use crate::template_recipes::{
    BaseRecipe, CabinetSize, DrawersRecipe, RecipeCandidate, RecipeError, RecipeMaterial,
    WallRecipe, base, drawers, wall,
};
use crate::units::{Conversion, Length, Unit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateKind {
    Base,
    Wall,
    Drawers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum MaterialRole {
    Carcass,
    Back,
    Box,
    BoxBottom,
    ExternalFront,
}

impl TemplateKind {
    pub fn roles(self) -> &'static [MaterialRole] {
        use MaterialRole::*;
        match self {
            Self::Base | Self::Wall => &[Carcass, Back],
            Self::Drawers => &[Carcass, Back, Box, BoxBottom, ExternalFront],
        }
    }

    pub fn fields(self) -> &'static [TemplateField] {
        use TemplateField::*;
        match self {
            Self::Base => &[Width, Depth, Height, RailWidth],
            Self::Wall => &[Width, Depth, Height, ShelfHeight],
            Self::Drawers => &[
                Width,
                Depth,
                Height,
                BoxDepth,
                SideClearance,
                RearClearance,
                VerticalClearance,
                FrontReveal,
                FrontGap,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TemplateField {
    Width,
    Depth,
    Height,
    RailWidth,
    ShelfHeight,
    BoxDepth,
    SideClearance,
    RearClearance,
    VerticalClearance,
    FrontReveal,
    FrontGap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProposedLength {
    pub conversion: Conversion,
    /// Consent applies to this exact proposal only. Replacing a value resets it.
    pub rounding_confirmed: bool,
}

impl ProposedLength {
    pub fn new(conversion: Conversion) -> Self {
        Self {
            conversion,
            rounding_confirmed: false,
        }
    }

    pub fn confirm_rounding(&mut self) {
        self.rounding_confirmed = true;
    }

    fn accepted(self) -> Option<Length> {
        match self.conversion {
            Conversion::Exact(value) => Some(value),
            Conversion::NeedsConfirmation(value) if self.rounding_confirmed => Some(value),
            Conversion::NeedsConfirmation(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DraftMaterial {
    /// Draft-only identity; replaced by a new portable UUID on each generation.
    pub id: Uuid,
    pub name: String,
    pub thickness: ProposedLength,
    pub grain: BoardGrain,
    pub color: Option<SrgbColor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupError {
    ProjectName,
    MaterialName(Uuid),
    DuplicateMaterial(Uuid),
    MaterialThickness(Uuid),
    MaterialRounding(Uuid),
    Rounding(TemplateField),
    MissingField(TemplateField),
    MissingCount,
    MissingRole(MaterialRole),
    UnknownMaterial(MaterialRole),
    Geometry(Vec<RecipeError>),
    InvalidProject(DomainError),
}

/// Candidate BOM, assumptions and allocation outlook are read-only. The UUIDs
/// in this preview are disposable; callers must never persist or select them.
#[derive(Clone, Debug)]
pub struct TemplateReview {
    pub candidate: RecipeCandidate,
    /// Y=0 is the carcass front. The external drawer front projects toward -Y;
    /// the overlay back's outside face is at `back_outside`.
    pub datums: TemplateDatums,
    pub materials: Vec<Material>,
    pub colors: BTreeMap<Uuid, SrgbColor>,
    /// In candidate board order; a fresh Welcome project has no declared stock.
    pub fits: Vec<(FirstFit, Option<crate::domain::Allocation>)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplateDatums {
    pub front_outside: Length,
    pub carcass_front: Length,
    pub carcass_rear: Length,
    pub back_outside: Length,
}

/// The setup is independent of the currently open document and its materials.
/// Nested material editing can work on a separate draft and call `add_material`
/// only on acceptance; cancelling it leaves this entire setup intact.
#[derive(Clone, Debug)]
pub struct TemplateSetup {
    pub kind: TemplateKind,
    pub project_name: String,
    pub currency: Currency,
    pub input_unit: Unit,
    pub materials: Vec<DraftMaterial>,
    pub roles: BTreeMap<MaterialRole, Uuid>,
    pub dimensions: BTreeMap<TemplateField, ProposedLength>,
    pub drawer_count: Option<usize>,
}

impl TemplateSetup {
    pub fn new(
        kind: TemplateKind,
        name: impl Into<String>,
        currency: Currency,
        unit: Unit,
    ) -> Self {
        Self {
            kind,
            project_name: name.into(),
            currency,
            input_unit: unit,
            materials: Vec::new(),
            roles: BTreeMap::new(),
            dimensions: BTreeMap::new(),
            drawer_count: None,
        }
    }

    pub fn add_material(
        &mut self,
        name: impl Into<String>,
        thickness: ProposedLength,
        grain: BoardGrain,
        color: Option<SrgbColor>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.materials.push(DraftMaterial {
            id,
            name: name.into(),
            thickness,
            grain,
            color,
        });
        id
    }

    /// Validate the entire setup before showing a candidate. A fresh project
    /// has no stock and never inherits another document's materials or stock.
    pub fn review(&self) -> Result<TemplateReview, Vec<SetupError>> {
        let mut errors = Vec::new();
        if self.project_name.trim().is_empty() || self.project_name.len() > 256 {
            errors.push(SetupError::ProjectName);
        }
        let mut materials = Vec::new();
        let mut colors = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for m in &self.materials {
            if !ids.insert(m.id) {
                errors.push(SetupError::DuplicateMaterial(m.id));
            }
            if m.name.trim().is_empty() || m.name.len() > 256 {
                errors.push(SetupError::MaterialName(m.id));
            }
            match m.thickness.accepted() {
                Some(value) if value.micrometres() > 0 => {
                    materials.push(Material {
                        id: m.id,
                        name: m.name.clone(),
                        default_thickness: value,
                        default_grain: m.grain,
                    });
                    if let Some(color) = m.color {
                        colors.insert(m.id, color);
                    }
                }
                None => errors.push(SetupError::MaterialRounding(m.id)),
                _ => errors.push(SetupError::MaterialThickness(m.id)),
            }
        }
        let mut lengths = BTreeMap::new();
        for field in self.kind.fields() {
            match self.dimensions.get(field) {
                None => errors.push(SetupError::MissingField(*field)),
                Some(proposal) => match proposal.accepted() {
                    Some(value) => {
                        lengths.insert(*field, value);
                    }
                    None => errors.push(SetupError::Rounding(*field)),
                },
            }
        }
        let mut chosen = HashMap::new();
        for role in self.kind.roles() {
            match self.roles.get(role) {
                None => errors.push(SetupError::MissingRole(*role)),
                Some(id) => match materials.iter().find(|m| m.id == *id) {
                    None => errors.push(SetupError::UnknownMaterial(*role)),
                    Some(m) => {
                        chosen.insert(
                            *role,
                            RecipeMaterial {
                                id: m.id,
                                thickness: m.default_thickness,
                            },
                        );
                    }
                },
            }
        }
        if self.kind == TemplateKind::Drawers && self.drawer_count.is_none() {
            errors.push(SetupError::MissingCount);
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let get = |f: TemplateField| lengths[&f];
        let mat = |r: MaterialRole| chosen[&r];
        let size = CabinetSize {
            width: get(TemplateField::Width),
            depth: get(TemplateField::Depth),
            height: get(TemplateField::Height),
        };
        let front = if self.kind == TemplateKind::Drawers {
            mat(MaterialRole::ExternalFront).thickness
        } else {
            Length::ZERO
        };
        let back = mat(MaterialRole::Back).thickness;
        let candidate = match self.kind {
            TemplateKind::Base => base(BaseRecipe {
                size,
                carcass: mat(MaterialRole::Carcass),
                back: mat(MaterialRole::Back),
                rail_width: get(TemplateField::RailWidth),
            }),
            TemplateKind::Wall => wall(WallRecipe {
                size,
                carcass: mat(MaterialRole::Carcass),
                back: mat(MaterialRole::Back),
                shelf_height: get(TemplateField::ShelfHeight),
            }),
            TemplateKind::Drawers => drawers(DrawersRecipe {
                size,
                carcass: mat(MaterialRole::Carcass),
                back: mat(MaterialRole::Back),
                box_material: mat(MaterialRole::Box),
                box_bottom: mat(MaterialRole::BoxBottom),
                external_front: mat(MaterialRole::ExternalFront),
                count: self.drawer_count.unwrap(),
                box_depth: get(TemplateField::BoxDepth),
                side_clearance: get(TemplateField::SideClearance),
                rear_clearance: get(TemplateField::RearClearance),
                vertical_clearance: get(TemplateField::VerticalClearance),
                front_reveal: get(TemplateField::FrontReveal),
                front_gap: get(TemplateField::FrontGap),
            }),
        }
        .map_err(|e| vec![SetupError::Geometry(e)])?;
        // The recipe has checked the positive input range and derived depth.
        let datums = TemplateDatums {
            front_outside: Length::from_micrometres(-front.micrometres()),
            carcass_front: Length::ZERO,
            carcass_rear: Length::from_micrometres(
                size.depth.micrometres() - front.micrometres() - back.micrometres(),
            ),
            back_outside: Length::from_micrometres(size.depth.micrometres() - front.micrometres()),
        };

        let mut disposable = Project::new(&self.project_name, self.currency);
        disposable.display_unit = self.input_unit;
        disposable.materials = materials.clone();
        disposable.material_colors = colors.clone();
        disposable.assemblies = candidate.assemblies.clone();
        let mut fits = Vec::with_capacity(candidate.boards.len());
        for board in &candidate.boards {
            disposable.boards.push(board.clone());
            let fit = allocate_new_board(&mut disposable, board.id);
            let allocation = disposable
                .allocations
                .iter()
                .find(|a| a.board_id == board.id)
                .cloned();
            fits.push((fit, allocation));
        }
        disposable
            .validate()
            .map_err(|e| vec![SetupError::InvalidProject(e)])?;
        Ok(TemplateReview {
            candidate,
            datums,
            materials,
            colors,
            fits,
        })
    }

    /// Call only after the host has resolved pending edit and document replacement
    /// guards. Build the new editor off to the side; a failure cannot replace the
    /// previous document. Fresh material/assembly/board/allocation IDs and the
    /// one transaction are owned by this operation. The result remains unsaved.
    pub fn generate(&self) -> Result<GeneratedTemplate, GenerateError> {
        let mut review = self.review().map_err(GenerateError::Setup)?;
        let mut fresh = Project::new(&self.project_name, self.currency);
        fresh.display_unit = self.input_unit;
        let mut editor = ProjectEditor::new(fresh).map_err(GenerateError::Project)?;
        let mut remap = HashMap::new();
        for material in &mut review.materials {
            let old = material.id;
            material.id = Uuid::new_v4();
            remap.insert(old, material.id);
        }
        for board in &mut review.candidate.boards {
            board.material_id = remap[&board.material_id];
        }
        review.colors = review
            .colors
            .into_iter()
            .map(|(id, color)| (remap[&id], color))
            .collect();
        let assembly_id = review.candidate.assemblies[0].id;
        let mut fits = Vec::new();
        editor
            .transact(|p| -> Result<(), ()> {
                p.materials.extend(review.materials);
                p.material_colors.extend(review.colors);
                p.assemblies.extend(review.candidate.assemblies);
                for board in review.candidate.boards {
                    let id = board.id;
                    p.boards.push(board);
                    fits.push((id, allocate_new_board(p, id)));
                }
                Ok(())
            })
            .map_err(GenerateError::Edit)?;
        Ok(GeneratedTemplate {
            editor,
            assembly_id,
            fits,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GenerateError {
    Setup(Vec<SetupError>),
    Project(DomainError),
    Edit(EditError<()>),
}

pub struct GeneratedTemplate {
    pub editor: ProjectEditor,
    pub assembly_id: Uuid,
    /// Generated board order and actual independent first-fit outcomes.
    pub fits: Vec<(Uuid, FirstFit)>,
}
