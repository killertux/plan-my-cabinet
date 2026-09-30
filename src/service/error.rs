//! One error shape for agents: a stable `code`, an English `message`, an
//! optional `hint` saying what to do next, and structured `details`. Every
//! library error converts here with an exhaustive match, so a new variant
//! fails to compile until it is described.
use std::convert::Infallible;

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

use crate::assembly_edit::AssemblyEditError;
use crate::board_commands::{BoardField, CreationError, RenameError};
use crate::board_dimensions::{BatchDimensionError, DimensionEditError};
use crate::catalog::hardware_catalog::CatalogEditError;
use crate::catalog::hinge_installation::{FitError, InstallationEditError, InstallationIssue};
use crate::color_commands::ColorEditError;
use crate::commands::EditError;
use crate::dimension_input::InputError;
use crate::domain::DomainError;
use crate::door_joint::JointError;
use crate::material_changes::{MaterialChangeError, MaterialDeleteError};
use crate::money::MoneyError;
use crate::optimization_worker::{ApplyError, WorkerError};
use crate::persistence::{PersistenceError, SaveError};
use crate::placement::PlacementError;
use crate::sheet_edit::{PlacementIssue, SheetDiagnostic, SheetEditError, SheetStatus};
use crate::stock_commands::{CurrencyChangeError, StockError, StockField};
use crate::template_recipes::{RecipeError, RecipeErrorKind};
use crate::template_setup::{GenerateError, SetupError};
use crate::units::UnitError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NoOpenProject,
    UnsavedChanges,
    NotFound,
    AmbiguousRef,
    InvalidArgument,
    InvalidLength,
    RoundingRequired,
    InvalidPose,
    InvalidProject,
    MaterialInUse,
    AllocatedStock,
    RevisionMismatch,
    StalePreview,
    StaleSearch,
    InvalidPlacement,
    Locked,
    Conflict,
    OptimizerTimeout,
    OptimizerFailed,
    CatalogError,
    InstallationError,
    JointError,
    TemplateError,
    RenderError,
    Io,
    Persistence,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ServiceError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

impl ServiceError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            details: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn not_found(kind: &str, reference: impl std::fmt::Display) -> Self {
        Self::new(
            ErrorCode::NotFound,
            format!("No {kind} matches '{reference}'."),
        )
        .hint("Use find_objects or the list_* tools to see ids and names.")
        .details(json!({ "kind": kind, "ref": reference.to_string() }))
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    /// Prefix the message with the field or argument it concerns.
    pub fn for_field(mut self, field: &str) -> Self {
        self.message = format!("{field}: {}", self.message);
        self
    }
}

pub type ServiceResult<T> = Result<T, ServiceError>;

pub fn unit_text(error: UnitError) -> &'static str {
    match error {
        UnitError::InvalidNumber => "not a valid number",
        UnitError::InvalidFraction => "not a valid inch fraction",
        UnitError::Overflow => "the value is too large",
        UnitError::NonPositiveDimension => "must be greater than zero",
        UnitError::OutOfBounds => "outside the ±1 km world limit",
        UnitError::NonFinite => "not a finite number",
        UnitError::InvalidRotation => "not a valid rotation",
    }
}

impl From<UnitError> for ServiceError {
    fn from(error: UnitError) -> Self {
        let code = match error {
            UnitError::OutOfBounds | UnitError::InvalidRotation => ErrorCode::InvalidPose,
            _ => ErrorCode::InvalidLength,
        };
        Self::new(code, unit_text(error))
    }
}

impl From<InputError> for ServiceError {
    fn from(error: InputError) -> Self {
        let message = match error {
            InputError::GroupingSeparators => {
                "digit grouping is not allowed; write 1234.5, not 1,234.5 or 1 234".into()
            }
            InputError::InvalidNumber => "not a valid length".into(),
            InputError::InvalidFraction => {
                "not a valid fraction (use a proper fraction like 5/8)".into()
            }
            InputError::FractionRequiresInches => "fractions are only allowed in inches".into(),
            InputError::Unit(e) => unit_text(e).to_owned(),
        };
        Self::new(ErrorCode::InvalidLength, message)
            .hint("Give a number (millimetres) or a string like \"600\", \"60 cm\", \"23 5/8 in\" or \"2'\".")
    }
}

impl From<MoneyError> for ServiceError {
    fn from(error: MoneyError) -> Self {
        let message = match error {
            MoneyError::InvalidAmount => {
                "not a valid amount; use 12.34 or \"12,34\" with at most 2 decimals"
            }
            MoneyError::NegativeAmount => "amounts cannot be negative",
            MoneyError::Overflow => "the amount is too large",
            MoneyError::CurrencyMismatch => "the amount is not in the project currency",
            MoneyError::InvalidStockIndex => "unknown stock piece in the price list",
            MoneyError::DuplicateStock => "a stock piece is listed twice",
            MoneyError::ReplacementCountMismatch | MoneyError::MissingReplacement => {
                "every stock piece needs a replacement price"
            }
        };
        Self::invalid(message)
    }
}

impl From<DomainError> for ServiceError {
    fn from(error: DomainError) -> Self {
        let message = match error {
            DomainError::UnsupportedVersion(v) => format!("unsupported project version {v}"),
            DomainError::DuplicateId(id) => format!("id {id} is used twice"),
            DomainError::DanglingReference { owner, target } => {
                format!("{owner} refers to {target}, which does not exist")
            }
            DomainError::HierarchyCycle(id) => format!("assembly {id} would contain itself"),
            DomainError::InvalidDimension(id) => format!("{id} has a zero or negative dimension"),
            DomainError::InvalidGridSpacing(e) => format!("grid spacing {}", unit_text(e)),
            DomainError::InvalidCuttingKerf(e) => format!("cutting kerf {}", unit_text(e)),
            DomainError::InvalidKerfConfirmationDate => "invalid kerf confirmation date".into(),
            DomainError::InvalidPose { owner, reason } => {
                format!("{owner} has an invalid position: {}", unit_text(reason))
            }
            DomainError::InvalidPrice(id) => {
                format!("stock {id} has a negative price or one in another currency")
            }
            DomainError::InvalidCutFee => "the cut fee is negative or in another currency".into(),
            DomainError::DuplicateAllocation(id) => format!("board {id} is placed twice"),
            DomainError::InvalidExportRecord => "an export record is invalid".into(),
            DomainError::InvalidCatalog(id) => format!("catalog entry {id} is invalid"),
            DomainError::SameHingeBoards(id) => {
                format!("hinge {id} uses the same board as door and mount")
            }
            DomainError::InvalidDoorJoint(id) => {
                format!("door {id} is inconsistent with its hinges")
            }
            DomainError::InvalidStockAlias => "stock labels are inconsistent".into(),
        };
        Self::new(
            ErrorCode::InvalidProject,
            format!("The change would make the project invalid: {message}."),
        )
    }
}

impl<E: Into<ServiceError>> From<EditError<E>> for ServiceError {
    fn from(error: EditError<E>) -> Self {
        match error {
            EditError::Command(e) => e.into(),
            EditError::InvalidProject(e) => e.into(),
            EditError::ProjectIdentityChanged => {
                Self::internal("the edit changed the project identity")
            }
            EditError::RevisionExhausted => Self::internal("the revision counter is exhausted"),
            EditError::NoPreview => Self::internal("no preview is active"),
        }
    }
}

impl From<()> for ServiceError {
    fn from((): ()) -> Self {
        Self::new(ErrorCode::Conflict, "the change was rejected")
    }
}

impl From<Infallible> for ServiceError {
    fn from(error: Infallible) -> Self {
        match error {}
    }
}

fn board_field(field: BoardField) -> &'static str {
    match field {
        BoardField::Material => "material",
        BoardField::Thickness => "thickness",
        BoardField::Length => "length",
        BoardField::Width => "width",
        BoardField::Pose => "pose",
    }
}

impl From<CreationError> for ServiceError {
    fn from(error: CreationError) -> Self {
        match error {
            CreationError::MissingMaterial(id) => Self::not_found("material", id),
            CreationError::MissingParent(id) => Self::not_found("assembly", id),
            CreationError::InvalidField(field, e) => Self::from(e).for_field(board_field(field)),
        }
    }
}

impl From<RenameError> for ServiceError {
    fn from(error: RenameError) -> Self {
        match error {
            RenameError::EmptyName => Self::invalid("the name is empty"),
            RenameError::TooLong => Self::invalid("the name is longer than 256 bytes"),
            RenameError::MissingObject(id) => Self::not_found("board, assembly or hardware", id),
        }
    }
}

impl From<ColorEditError> for ServiceError {
    fn from(error: ColorEditError) -> Self {
        match error {
            ColorEditError::MissingMaterial(id) => Self::not_found("material", id),
        }
    }
}

impl From<DimensionEditError> for ServiceError {
    fn from(error: DimensionEditError) -> Self {
        match error {
            DimensionEditError::MissingBoard(id) => Self::not_found("board", id),
            DimensionEditError::InvalidDimension(e) => Self::from(e).for_field("dimension"),
            DimensionEditError::InvalidPose(e) => Self::new(
                ErrorCode::InvalidPose,
                format!("the resized board would leave the world: {}", unit_text(e)),
            ),
            DimensionEditError::StalePreview => stale(),
        }
    }
}

fn stale() -> ServiceError {
    ServiceError::new(
        ErrorCode::StalePreview,
        "the project changed while the edit was prepared",
    )
    .hint("Repeat the call.")
}

impl From<BatchDimensionError> for ServiceError {
    fn from(error: BatchDimensionError) -> Self {
        match error {
            BatchDimensionError::MissingSelection(id) => Self::not_found("board or assembly", id),
            BatchDimensionError::EmptySelection => Self::invalid("the selection has no boards"),
            BatchDimensionError::MissingAnchor(id) => {
                Self::invalid(format!("board {id} has no anchor"))
            }
            BatchDimensionError::Target { board_id, reason } => {
                let mut e = Self::from(reason);
                e.message = format!("board {board_id}: {}", e.message);
                e
            }
            BatchDimensionError::StalePreview => stale(),
        }
    }
}

impl From<MaterialChangeError> for ServiceError {
    fn from(error: MaterialChangeError) -> Self {
        match error {
            MaterialChangeError::MissingMaterial(id) => Self::not_found("material", id),
            MaterialChangeError::MissingBoard(id) => Self::not_found("board", id),
            MaterialChangeError::InvalidThickness(e) => Self::from(e).for_field("thickness"),
            MaterialChangeError::InvalidPose { board_id, reason } => Self::new(
                ErrorCode::InvalidPose,
                format!(
                    "board {board_id} would leave the world: {}",
                    unit_text(reason)
                ),
            ),
            MaterialChangeError::StalePreview => stale(),
            MaterialChangeError::DuplicateSelection(id) => {
                Self::invalid(format!("board {id} is selected twice"))
            }
            MaterialChangeError::NotDependent(id) => Self::invalid(format!(
                "board {id} does not follow this material's default thickness or grain"
            )),
        }
    }
}

impl From<MaterialDeleteError> for ServiceError {
    fn from(error: MaterialDeleteError) -> Self {
        match error {
            MaterialDeleteError::MissingMaterial(id) => Self::not_found("material", id),
            MaterialDeleteError::InUse { boards, stock } => Self::new(
                ErrorCode::MaterialInUse,
                format!(
                    "the material is used by {} board(s) and {} stock piece(s)",
                    boards.len(),
                    stock.len()
                ),
            )
            .hint("Assign the boards another material (set_board_material) and delete the stock first.")
            .details(json!({ "boards": boards, "stock": stock })),
        }
    }
}

impl From<AssemblyEditError> for ServiceError {
    fn from(error: AssemblyEditError) -> Self {
        match error {
            AssemblyEditError::MissingObject(id) => Self::not_found("object", id),
            AssemblyEditError::MissingParent(id) => Self::not_found("assembly", id),
            AssemblyEditError::EmptySelection => Self::invalid("no objects were given"),
            AssemblyEditError::Cycle(id) => {
                Self::invalid(format!("{id} cannot be placed inside its own descendant"))
            }
            AssemblyEditError::InvalidPose(e) => Self::from(e).for_field("pose"),
            AssemblyEditError::PoseNotPreserved(id) => Self::new(
                ErrorCode::InvalidPose,
                format!("{id} cannot keep its world position under the new parent"),
            ),
            AssemblyEditError::InvalidDimensions => Self::new(
                ErrorCode::InvalidLength,
                "hardware dimensions must be greater than zero",
            ),
        }
    }
}

impl From<PlacementError> for ServiceError {
    fn from(error: PlacementError) -> Self {
        match error {
            PlacementError::MissingBoard(id) => Self::not_found("board", id),
            PlacementError::MissingParent(id) => Self::not_found("assembly", id),
            PlacementError::InvalidFace => {
                Self::invalid("invalid face; use -x, +x, -y, +y, -z or +z")
            }
            PlacementError::SameBoard => Self::invalid("a board cannot be placed against itself"),
            PlacementError::InvalidRotation => Self::new(
                ErrorCode::InvalidPose,
                "rotations must be finite and within ±360°",
            ),
            PlacementError::OffGrid => Self::new(
                ErrorCode::InvalidPose,
                "positions must be whole micrometres (at most 3 decimals in mm)",
            ),
            PlacementError::InvalidPose(e) => Self::from(e).for_field("pose"),
        }
    }
}

fn stock_field(field: StockField) -> &'static str {
    match field {
        StockField::Material => "material",
        StockField::Length => "length",
        StockField::Width => "width",
        StockField::Thickness => "thickness",
        StockField::Trim => "trim (must leave usable area on both axes)",
        StockField::Price => "price (non-negative, in the project currency)",
        StockField::Quantity => "quantity (1 to 10000)",
        StockField::Priority => "priority",
    }
}

impl From<StockError> for ServiceError {
    fn from(error: StockError) -> Self {
        match error {
            StockError::MissingStock(id) => Self::not_found("stock piece", id),
            StockError::MissingMaterial(id) => Self::not_found("material", id),
            StockError::Invalid(field) => Self::invalid(format!("invalid {}", stock_field(field))),
            StockError::PriorityExhausted => Self::internal("stock priorities are exhausted"),
            StockError::AllocatedStock(id) => Self::new(
                ErrorCode::AllocatedStock,
                format!("stock piece {id} still has parts placed on it"),
            )
            .hint("Pass unallocate_first: true, or move the parts first."),
        }
    }
}

impl From<CurrencyChangeError> for ServiceError {
    fn from(error: CurrencyChangeError) -> Self {
        match error {
            CurrencyChangeError::DuplicateStock(id) => {
                Self::invalid(format!("stock {id} is listed twice"))
            }
            CurrencyChangeError::UnknownStock(id) => Self::not_found("stock piece", id),
            CurrencyChangeError::MissingStock(id) => {
                Self::invalid(format!("stock {id} needs a price in the new currency"))
            }
            CurrencyChangeError::Money(e) => e.into(),
        }
    }
}

pub fn sheet_diagnostics_json(
    project: &crate::domain::Project,
    diagnostics: &[SheetDiagnostic],
) -> Value {
    Value::Array(
        diagnostics
            .iter()
            .map(|d| {
                let alias = project.stock_alias(d.stock_id).map(str::to_owned);
                let (status, issue) = match &d.status {
                    SheetStatus::Verified(tree) => ("verified", json!({ "cuts": tree.cut_count() })),
                    SheetStatus::Violation(issue) => ("violation", match issue {
                        PlacementIssue::Overlap(a, b) => json!({
                            "kind": "overlap",
                            "boards": [a, b],
                            "names": [board_name(project, *a), board_name(project, *b)],
                        }),
                        PlacementIssue::KerfOrCutSequence => json!({
                            "kind": "kerf_or_cut_sequence",
                            "detail": "parts are too close for the saw kerf, or no straight full-length cut sequence separates them",
                        }),
                        PlacementIssue::Rule(rule) => json!({ "kind": "rule", "detail": format!("{rule:?}") }),
                    }),
                    SheetStatus::Exhausted => ("search_exhausted", Value::Null),
                };
                json!({ "stock_id": d.stock_id, "stock_alias": alias, "status": status, "issue": issue })
            })
            .collect(),
    )
}

fn board_name(project: &crate::domain::Project, id: uuid::Uuid) -> String {
    project
        .board(id)
        .map(|b| b.name.clone())
        .unwrap_or_default()
}

impl From<SheetEditError> for ServiceError {
    fn from(error: SheetEditError) -> Self {
        match error {
            SheetEditError::MissingBoard(id) => Self::not_found("board", id),
            SheetEditError::MissingStock(id) => Self::not_found("stock piece", id),
            SheetEditError::Locked(id) => Self::new(ErrorCode::Locked, format!("board {id} is locked on its sheet"))
                .hint("Unlock it first with a {op: \"lock\", locked: false} operation."),
            SheetEditError::NotAllocated(id) => Self::invalid(format!("board {id} is not placed on a sheet")),
            SheetEditError::InvalidPlacement(diagnostics) => Self::new(
                ErrorCode::InvalidPlacement,
                "the placement cannot be cut: a sheet has overlapping parts or no valid guillotine cut sequence",
            )
            .details(json!({ "sheets": diagnostics.len() })),
            SheetEditError::Closed => Self::internal("the sheet edit session is closed"),
        }
    }
}

pub fn installation_issue_text(issue: &InstallationIssue) -> &'static str {
    match issue {
        InstallationIssue::MissingPart(_) => "the door or mounting board is missing",
        InstallationIssue::MissingCatalog(_) => "the catalog entry is missing",
        InstallationIssue::MissingVerifiedCatalog => {
            "the catalog entry has no verified installation data"
        }
        InstallationIssue::UnsupportedThickness => {
            "the door thickness is outside the hinge's range"
        }
        InstallationIssue::UnsupportedOverlay => "the K/R (or F) pair is not in the catalog table",
        InstallationIssue::CupOutsideDoor => "the cup hole falls outside the door",
        InstallationIssue::PlateOutsideMount => "the mounting plate falls outside the side",
        InstallationIssue::InsetShallowerThanDoor => {
            "inset depth E is smaller than the door thickness; the door would stand proud"
        }
    }
}

impl From<InstallationEditError> for ServiceError {
    fn from(error: InstallationEditError) -> Self {
        match error {
            InstallationEditError::MissingInstallation => Self::not_found("hinge", "given id"),
            InstallationEditError::InvalidDistance => Self::new(
                ErrorCode::InstallationError,
                "hinge distances must be zero or more",
            ),
            InstallationEditError::IncompatibleJoint => Self::new(
                ErrorCode::InstallationError,
                "the change would break the door relationship that uses this hinge",
            )
            .hint("Remove the door (remove_door) first, then create it again."),
        }
    }
}

impl From<FitError> for ServiceError {
    fn from(error: FitError) -> Self {
        let message = match error {
            FitError::MissingPart => "the door or mounting board is missing",
            FitError::NotParallel => "no door edge is parallel to the mounting board's face",
            FitError::OutsideMount => {
                "the hinge position falls beyond the ends of the mounting board"
            }
        };
        Self::new(ErrorCode::InstallationError, message)
            .hint("Check the door and mount with describe_scene; the door must lie next to the side it hinges on.")
    }
}

impl From<JointError> for ServiceError {
    fn from(error: JointError) -> Self {
        let message = match error {
            JointError::MissingRoot => "the door board or assembly does not exist",
            JointError::MissingMount => "the mounting board does not exist",
            JointError::MissingHinge(_) => "a listed hinge does not exist",
            JointError::EmptyHinges => "a door needs at least one hinge",
            JointError::IncompatibleHinge(_) => {
                "all hinges must share the door, catalog entry, side and mounting board"
            }
            JointError::OverlappingRoot => "the moving part overlaps another door",
            JointError::Cycle => "the moving part contains its mounting board",
            JointError::InvalidAxis => "the hinges do not define a valid rotation axis",
            JointError::InvalidPose => "the door pose is invalid",
            JointError::StalePreview => "the project changed while the door was prepared",
            JointError::MissingJoint => "the door relationship does not exist",
            JointError::UnverifiedLimit => "the hinge's opening limit is not verified",
            JointError::NeedsReview => "the door relationship needs review; recreate it",
            JointError::InvalidAngle => "the angle is outside the hinge's opening range",
        };
        Self::new(ErrorCode::JointError, message).details(json!({ "reason": format!("{error:?}") }))
    }
}

impl From<CatalogEditError> for ServiceError {
    fn from(error: CatalogEditError) -> Self {
        match error {
            CatalogEditError::MissingEntry => Self::not_found("catalog entry", "given id"),
            CatalogEditError::NotInCatalog => Self::new(
                ErrorCode::CatalogError,
                "no loaded catalog pack has a record for this entry",
            ),
            CatalogEditError::DependentInstallation(reason) => Self::new(
                ErrorCode::CatalogError,
                format!("an installed hinge depends on the current record: {reason}"),
            ),
        }
    }
}

impl From<WorkerError> for ServiceError {
    fn from(error: WorkerError) -> Self {
        match error {
            WorkerError::Timeout => Self::new(
                ErrorCode::OptimizerTimeout,
                "the search did not finish in time",
            )
            .hint("Increase max_seconds or reduce the budget."),
            WorkerError::Cancelled => {
                Self::new(ErrorCode::OptimizerFailed, "the search was cancelled")
            }
            WorkerError::Generation(e) => Self::new(
                ErrorCode::OptimizerFailed,
                format!("no complete plan could be generated: {e:?}"),
            )
            .hint("Run get_diagnostics: every board needs a matching sheet it fits on."),
            WorkerError::Ranking(e) => {
                Self::new(ErrorCode::OptimizerFailed, format!("ranking failed: {e:?}"))
            }
            WorkerError::Disconnected => Self::internal("the search thread stopped unexpectedly"),
        }
    }
}

impl From<ApplyError> for ServiceError {
    fn from(error: ApplyError) -> Self {
        match error {
            ApplyError::Cancelled => Self::new(ErrorCode::StaleSearch, "the search was cancelled"),
            ApplyError::DifferentProject => Self::new(
                ErrorCode::StaleSearch,
                "the search belongs to another project",
            ),
            ApplyError::Stale { started, current } => Self::new(
                ErrorCode::StaleSearch,
                format!("the project changed since the search (revision {started} → {current})"),
            )
            .hint("Run optimize_cut_plan again."),
            ApplyError::UnknownCandidate => Self::invalid("no candidate has that index"),
            ApplyError::InvalidCandidate(e) => Self::new(
                ErrorCode::StaleSearch,
                format!("the candidate no longer fits the current locks and stock: {e:?}"),
            ),
            ApplyError::Edit(e) => e.into(),
        }
    }
}

impl From<PersistenceError> for ServiceError {
    fn from(error: PersistenceError) -> Self {
        let code = match error {
            PersistenceError::Io(_) => ErrorCode::Io,
            _ => ErrorCode::Persistence,
        };
        Self::new(code, error.to_string())
    }
}

impl From<SaveError> for ServiceError {
    fn from(error: SaveError) -> Self {
        Self::new(ErrorCode::Io, error.to_string())
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(error: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, error.to_string())
    }
}

fn recipe_text(error: &RecipeError) -> String {
    let kind = match error.kind {
        RecipeErrorKind::NonPositive => "must be greater than zero",
        RecipeErrorKind::Negative => "cannot be negative",
        RecipeErrorKind::Geometry => {
            "leaves no room for the parts (the cabinet is too small for these values)"
        }
        RecipeErrorKind::OutOfBounds => "is too large",
        RecipeErrorKind::TooMany => "is too many",
        RecipeErrorKind::MaterialConflict => "conflicts with the chosen material thicknesses",
    };
    format!("{} {kind}", error.field)
}

fn setup_text(error: &SetupError) -> String {
    match error {
        SetupError::ProjectName => "the project name is empty or longer than 256 bytes".into(),
        SetupError::MaterialName(_) => "a material name is empty or too long".into(),
        SetupError::DuplicateMaterial(_) => "a material is listed twice".into(),
        SetupError::MaterialThickness(_) => "a material thickness is not positive".into(),
        SetupError::MaterialRounding(_) => "a material thickness needs rounding consent".into(),
        SetupError::Rounding(field) => format!("{field:?} needs rounding consent"),
        SetupError::MissingField(field) => format!(
            "missing dimension {}",
            crate::service::dto::snake(&format!("{field:?}"))
        ),
        SetupError::MissingCount => "drawer_count is required for drawers".into(),
        SetupError::MissingRole(role) => format!("no material for role {role:?}"),
        SetupError::UnknownMaterial(role) => format!("the material for role {role:?} is unknown"),
        SetupError::Geometry(errors) => errors
            .iter()
            .map(recipe_text)
            .collect::<Vec<_>>()
            .join("; "),
        SetupError::InvalidProject(e) => ServiceError::from(e.clone()).message,
    }
}

impl From<GenerateError> for ServiceError {
    fn from(error: GenerateError) -> Self {
        match error {
            GenerateError::Setup(errors) => Self::new(
                ErrorCode::TemplateError,
                errors.iter().map(setup_text).collect::<Vec<_>>().join("; "),
            ),
            GenerateError::Project(e) => e.into(),
            GenerateError::Edit(e) => e.into(),
        }
    }
}
