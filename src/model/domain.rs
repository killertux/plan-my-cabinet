//! Portable project records and structural validation. Names are display-only;
//! UUIDs identify physical objects and are unique across all record kinds.
use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::export::ExportRecord;
pub use crate::hardware_spec::{
    CatalogItem, FootShape, FootSpec, Glide, Section, SlideExtension, SlideHole, SlideSpec,
};
use crate::money::{Currency, Money};
use crate::units::{Length, Pose, Unit, UnitError};

pub const SCHEMA_VERSION: u32 = 6;
pub const DEFAULT_GRID_SPACING: Length = Length::from_micrometres(10_000);
/// Provisional project cutting assumption; confirm against the actual saw before shop use.
pub const DEFAULT_CUTTING_KERF: Length = Length::from_micrometres(5_000);
/// A wider interval than the supported world extent cannot provide a useful grid.
pub const MAX_GRID_SPACING: Length = Length::from_micrometres(1_000_000_000);

pub fn validate_grid_spacing(spacing: Length) -> Result<(), UnitError> {
    spacing.positive()?;
    if spacing > MAX_GRID_SPACING {
        return Err(UnitError::OutOfBounds);
    }
    Ok(())
}

fn default_grid_spacing() -> Length {
    DEFAULT_GRID_SPACING
}

fn default_cutting_kerf() -> Length {
    DEFAULT_CUTTING_KERF
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardGrain {
    Length,
    Width,
    Unrestricted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StockGrain {
    AlongX,
    AlongY,
    Nondirectional,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StockSource {
    Owned,
    ToPurchase,
}

fn first_stock_alias() -> u64 {
    1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: Uuid,
    pub name: String,
    pub default_thickness: Length,
    pub default_grain: BoardGrain,
    /// What the sheet is made of. Only MDF and MDP take edge banding. Files
    /// older than schema 5 infer it from the name on load.
    #[serde(default)]
    pub kind: MaterialKind,
    /// The band automatic banding puts on this material's free edges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_band: Option<Uuid>,
    /// Which faces of the sheet are coated (melamine, laminate). Only MDF,
    /// MDP and HDF have a coating; other kinds ignore it. Files older than
    /// schema 6 infer it from the name on load.
    #[serde(default)]
    pub coating: Coating,
}

impl Material {
    /// The coating that applies: `None` for kinds that are never coated.
    pub fn effective_coating(&self) -> Option<Coating> {
        self.kind.accepts_coating().then_some(self.coating)
    }
}

/// How many broad faces of a sheet are coated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Coating {
    /// Raw on both faces ("MDF cru").
    None,
    /// One coated face, the other raw ("1 face").
    OneSide,
    #[default]
    BothSides,
}

impl Coating {
    pub const ALL: [Self; 3] = [Self::None, Self::OneSide, Self::BothSides];

    /// Best guess from a material name, for files that predate coatings:
    /// "cru"/"raw" is uncoated, "1 face"/"one side" is coated on one face,
    /// anything else on both.
    pub fn infer(name: &str) -> Self {
        let name = name.to_lowercase();
        let tokens: Vec<&str> = name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .collect();
        let has = |word: &str| tokens.contains(&word);
        let pair = |a: &str, b: &[&str]| tokens.windows(2).any(|w| w[0] == a && b.contains(&w[1]));
        if has("cru") || has("crua") || has("raw") || has("unfaced") {
            Self::None
        } else if has("1f")
            || pair("1", &["face", "lado", "side"])
            || pair("uma", &["face"])
            || pair("one", &["side", "sided", "face"])
        {
            Self::OneSide
        } else {
            Self::BothSides
        }
    }
}

/// Which broad face of a one-side coated board carries the coating.
/// `Auto` follows the automatic rule (the face that shows); the others are
/// the user's choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CoatedFace {
    #[default]
    Auto,
    MinZ,
    MaxZ,
}

impl CoatedFace {
    pub fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaterialKind {
    Mdf,
    Mdp,
    Hdf,
    Plywood,
    SolidWood,
    #[default]
    Other,
}

impl MaterialKind {
    pub const ALL: [Self; 6] = [
        Self::Mdf,
        Self::Mdp,
        Self::Hdf,
        Self::Plywood,
        Self::SolidWood,
        Self::Other,
    ];

    /// Edge banding is glued to the exposed particle or fibre core of MDF
    /// and MDP; other sheets are left as they are.
    pub const fn accepts_banding(self) -> bool {
        matches!(self, Self::Mdf | Self::Mdp)
    }

    /// Sheets sold coated on none, one or both faces.
    pub const fn accepts_coating(self) -> bool {
        matches!(self, Self::Mdf | Self::Mdp | Self::Hdf)
    }

    /// Best guess from a material name, for files that predate material kinds.
    pub fn infer(name: &str) -> Self {
        let name = name.to_lowercase();
        let has = |word: &str| {
            name.split(|c: char| !c.is_alphanumeric())
                .any(|token| token == word)
        };
        if has("mdf") {
            Self::Mdf
        } else if has("mdp") || has("aglomerado") || has("particleboard") {
            Self::Mdp
        } else if has("hdf") || has("eucatex") {
            Self::Hdf
        } else if has("compensado") || has("plywood") || has("naval") {
            Self::Plywood
        } else if has("madeira") || has("maciça") || has("maciço") || has("solid") {
            Self::SolidWood
        } else {
            Self::Other
        }
    }
}

/// A roll of edge band. The name is what a shop sees (for example
/// "Fita Branca 1x22"); thickness and height are the tape's own size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EdgeBand {
    pub id: Uuid,
    pub name: String,
    pub thickness: Length,
    pub height: Length,
    pub color: SrgbColor,
}

/// Banding on one board edge. `Auto` follows the automatic rule (the
/// material's default band on edges not joined to another board); `On` and
/// `Off` are the user's overrides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeBanding {
    #[default]
    Auto,
    On(Uuid),
    Off,
}

/// Per-edge banding, indexed like `BoardEdge::ALL` (MinX, MaxX, MinY, MaxY).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoardBanding(pub [EdgeBanding; 4]);

impl BoardBanding {
    pub fn is_automatic(&self) -> bool {
        self.0.iter().all(|edge| *edge == EdgeBanding::Auto)
    }

    pub fn get(&self, edge: BoardEdge) -> EdgeBanding {
        self.0[edge.index()]
    }

    pub fn set(&mut self, edge: BoardEdge, value: EdgeBanding) {
        self.0[edge.index()] = value;
    }

    pub fn bands(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.0.iter().filter_map(|edge| match edge {
            EdgeBanding::On(band) => Some(*band),
            _ => None,
        })
    }
}

/// Portable sRGB channels, independent of any renderer or display profile.
/// JSON represents this value as a three-element byte array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SrgbColor(pub [u8; 3]);

/// Appearance for materials without a selected color (including legacy files).
pub const NEUTRAL_MATERIAL_COLOR: SrgbColor = SrgbColor([200, 196, 187]);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub id: Uuid,
    pub name: String,
    pub material_id: Uuid,
    /// Blank dimensions along the board's local X/Y/Z axes.
    pub length: Length,
    pub width: Length,
    /// Snapshot of effective thickness; material defaults do not resize existing boards.
    pub thickness: Length,
    /// `None` follows the current material default; an explicit override survives default edits.
    pub grain_override: Option<BoardGrain>,
    pub parent_id: Option<Uuid>,
    pub pose: Pose,
    #[serde(default, skip_serializing_if = "BoardBanding::is_automatic")]
    pub banding: BoardBanding,
    /// On a one-side coated material: which face is coated.
    #[serde(default, skip_serializing_if = "CoatedFace::is_auto")]
    pub coated_face: CoatedFace,
}

impl Board {
    pub fn blank_dimensions(&self) -> [Length; 3] {
        [self.length, self.width, self.thickness]
    }

    pub fn effective_grain(&self, material: &Material) -> BoardGrain {
        self.grain_override.unwrap_or(material.default_grain)
    }

    /// Independent copy: only the parent/material references are shared, never identity or allocation.
    pub fn duplicate(&self) -> Self {
        Self {
            id: Uuid::new_v4(),
            ..self.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Assembly {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub pose: Pose,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stock {
    pub id: Uuid,
    pub name: String,
    pub material_id: Uuid,
    pub length: Length,
    pub width: Length,
    /// Measured thickness, independent of the material's current default.
    pub thickness: Length,
    pub grain: StockGrain,
    pub source: StockSource,
    pub price: Option<Money>,
    pub priority: u32,
    /// Total loss on each edge (left, right, bottom, top), in manufacturing units.
    pub trim: [Length; 4],
}

impl Stock {
    pub fn duplicate(&self) -> Self {
        Self {
            id: Uuid::new_v4(),
            ..self.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Allocation {
    pub id: Uuid,
    pub board_id: Uuid,
    pub stock_id: Uuid,
    pub origin: [Length; 2],
    pub quarter_turn: bool,
    pub locked: bool,
}

/// Project-pinned catalog facts, copied from a catalog pack when added. The
/// project keeps this snapshot: a changed or missing pack never alters it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogReference {
    pub id: Uuid,
    pub name: String,
    pub product_id: String,
    pub plate_id: Option<String>,
    pub source: String,
    pub revision: String,
    pub installation_dimensions: HashMap<String, Length>,
    /// Absent for legacy/user-authored references, which cannot claim supported guidance.
    #[serde(default)]
    pub verified_hinge: Option<VerifiedHinge>,
    /// The pack record this snapshot was copied from; absent for records
    /// created before catalog packs.
    #[serde(default)]
    pub origin: Option<CatalogOrigin>,
    /// Slide or foot facts; `None` for hinges (see `verified_hinge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<CatalogItem>,
}

impl CatalogReference {
    pub fn slide(&self) -> Option<&SlideSpec> {
        match &self.item {
            Some(CatalogItem::Slide(spec)) => Some(spec),
            _ => None,
        }
    }

    pub fn foot(&self) -> Option<&FootSpec> {
        match &self.item {
            Some(CatalogItem::Foot(spec)) => Some(spec),
            _ => None,
        }
    }
}

/// Where a pinned snapshot came from, used to offer updates and to name the
/// data's provenance on screen and in exports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogOrigin {
    pub pack_id: String,
    pub pack_version: String,
    pub manufacturer: String,
    pub item_id: String,
    pub variant_code: String,
}

/// Cup-arm geometry. Overlay arms cover the cabinet side by R; an inset arm
/// sits the door between the sides with a gap F.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HingeArm {
    #[default]
    FullOverlay,
    HalfOverlay,
    Inset,
}

impl HingeArm {
    pub const ALL: [Self; 3] = [Self::FullOverlay, Self::HalfOverlay, Self::Inset];

    pub const fn is_inset(self) -> bool {
        matches!(self, Self::Inset)
    }
}

/// Factual installation references for one catalog kit/plate pair.
/// Lengths are integer micrometres; K is measured from door edge to cup edge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedHinge {
    pub printed_page: u16,
    pub pdf_page: u16,
    pub source_sha256: String,
    pub attribution: String,
    #[serde(default)]
    pub arm: HingeArm,
    pub plate_height: Length,
    /// The manufacturer's K table. For overlay arms `overlay` is R; for
    /// `HingeArm::Inset` it is the gap F between door edge and cabinet side.
    pub overlay_by_cup_edge: Vec<OverlaySetting>,
    pub door_thickness_min: Length,
    pub door_thickness_max: Length,
    pub cup_diameter: Length,
    pub cup_depth: Length,
    pub plate_hole_pitch: Length,
    pub plate_front_offset: Length,
    pub opening_limit_degrees: u16,
    /// Fastener specifications and pilot dimensions were not verified.
    pub screw_details: UnavailableDetail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlaySetting {
    pub cup_edge_setback: Length,
    pub overlay: Length,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnavailableDetail {
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HardwareKind {
    /// Local X/Y/Z dimensions of a simple non-wooden placeholder.
    Placeholder {
        dimensions: [Length; 3],
    },
    Catalog {
        catalog_id: Uuid,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hardware {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub pose: Pose,
    pub kind: HardwareKind,
}

/// Board-local installation annotation, independent of assembly/world poses.
/// X is board length, Y width, Z thickness. Each instance is one physical hinge;
/// the number of instances is user supplied, never calculated from door load.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HingeInstallation {
    pub id: Uuid,
    pub door_board_id: Uuid,
    pub mounting_board_id: Uuid,
    pub catalog_id: Uuid,
    pub side: HingeMountingSide,
    /// Centreline positions measured from each board's local minimum Y edge.
    pub door_y: Length,
    pub mount_y: Length,
    /// K is the distance from the selected door X edge to the *cup edge*.
    pub cup_edge_setback: Length,
    /// R for overlay arms; the gap F for an inset arm.
    pub overlay: Length,
    /// Inset arms only: E, from the mounting board's front edge to the door's
    /// inside face. The plate sits at the catalog front offset plus E.
    #[serde(default)]
    pub inset_depth: Length,
}

/// Mechanical relationship, distinct from the assembly tree. The closed pose
/// and axis are world-space references captured on confirmation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DoorJoint {
    pub id: Uuid,
    pub moving_root_id: Uuid,
    pub mounting_board_id: Uuid,
    pub hinge_installation_ids: Vec<Uuid>,
    pub closed_world_pose: Pose,
    pub closed_local_pose: Pose,
    pub axis_origin_mm: [f64; 3],
    pub axis_direction: [f64; 3],
}

/// A board edge. MinX/MaxX run along local Y; MinY/MaxY run along local X.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardEdge {
    MinX,
    MaxX,
    MinY,
    MaxY,
}

impl BoardEdge {
    pub const ALL: [Self; 4] = [Self::MinX, Self::MaxX, Self::MinY, Self::MaxY];

    /// Local axis the edge runs along (0 = X, 1 = Y).
    /// Position in `ALL`, and in `BoardBanding`.
    pub const fn index(self) -> usize {
        match self {
            Self::MinX => 0,
            Self::MaxX => 1,
            Self::MinY => 2,
            Self::MaxY => 3,
        }
    }

    pub const fn along_axis(self) -> usize {
        match self {
            Self::MinX | Self::MaxX => 1,
            Self::MinY | Self::MaxY => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoardFace {
    MinZ,
    MaxZ,
}

/// A pair of drawer slides: one cabinet member on each carcass side, one
/// drawer member on each drawer box side. Index 0 is the left (smaller world
/// X) side. The drawer's pull direction is derived from the geometry, so no
/// closed pose is stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideInstallation {
    pub id: Uuid,
    pub catalog_id: Uuid,
    /// The board or assembly that moves with the drawer.
    pub drawer_root_id: Uuid,
    pub drawer_sides: [Uuid; 2],
    pub cabinet_sides: [Uuid; 2],
    pub sides: [SlideMountingSide; 2],
    /// Slide centre line above the drawer box side's bottom edge.
    pub height: Length,
    /// Cabinet member front, behind the carcass front edge.
    pub setback: Length,
}

/// Which edges and faces a slide uses on one side, fitted when installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideMountingSide {
    pub cabinet_front_edge: BoardEdge,
    pub cabinet_face: BoardFace,
    pub drawer_front_edge: BoardEdge,
    pub drawer_bottom_edge: BoardEdge,
    pub drawer_face: BoardFace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HingeMountingSide {
    pub door_edge: BoardEdge,
    pub door_face: BoardFace,
    /// Cabinet-front datum for the 37 mm plate reference.
    pub mount_front_edge: BoardEdge,
    pub mount_face: BoardFace,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub revision: u64,
    pub display_unit: Unit,
    #[serde(default = "default_grid_spacing")]
    pub grid_spacing: Length,
    #[serde(default = "default_cutting_kerf")]
    pub cutting_kerf: Length,
    /// Explicit shop assumption, bound to the exact kerf value (never inferred from a default).
    #[serde(default)]
    pub confirmed_shop_kerf: Option<Length>,
    /// UTC Unix milliseconds of an explicit confirmation of `confirmed_shop_kerf`.
    /// Absent for unconfirmed kerfs and for confirmations from older projects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_shop_kerf_unix_ms: Option<u64>,
    pub currency: Currency,
    /// Unknown until a shop charge is explicitly entered; zero is a known free cut.
    #[serde(default)]
    pub cut_fee: Option<Money>,
    pub materials: Vec<Material>,
    /// Optional appearance keyed by material identity. Empty maps are omitted so
    /// opening and explicitly saving an uncolored legacy file adds no color data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub material_colors: BTreeMap<Uuid, SrgbColor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_bands: Vec<EdgeBand>,
    pub boards: Vec<Board>,
    pub assemblies: Vec<Assembly>,
    pub stock: Vec<Stock>,
    /// Persistent aliases are keyed by physical identity, never by priority or
    /// current ownership. Removed identities leave their numbers reserved.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stock_aliases: BTreeMap<Uuid, String>,
    #[serde(default = "first_stock_alias")]
    pub next_stock_s_alias: u64,
    #[serde(default = "first_stock_alias")]
    pub next_stock_o_alias: u64,
    pub allocations: Vec<Allocation>,
    pub catalog: Vec<CatalogReference>,
    pub hardware: Vec<Hardware>,
    #[serde(default)]
    pub hinge_installations: Vec<HingeInstallation>,
    #[serde(default)]
    pub door_joints: Vec<DoorJoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slide_installations: Vec<SlideInstallation>,
    #[serde(default)]
    pub export_records: Vec<ExportRecord>,
    /// Part-list files written for shops (CorteCloud and later formats).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_exports: Vec<crate::formats::FileExportRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainError {
    UnsupportedVersion(u32),
    DuplicateId(Uuid),
    DanglingReference {
        owner: Uuid,
        target: Uuid,
    },
    HierarchyCycle(Uuid),
    InvalidDimension(Uuid),
    InvalidGridSpacing(UnitError),
    InvalidCuttingKerf(UnitError),
    InvalidKerfConfirmationDate,
    InvalidPose {
        owner: Uuid,
        reason: UnitError,
    },
    InvalidPrice(Uuid),
    InvalidCutFee,
    DuplicateAllocation(Uuid),
    InvalidExportRecord,
    InvalidCatalog(Uuid),
    SameHingeBoards(Uuid),
    InvalidDoorJoint(Uuid),
    InvalidSlide(Uuid),
    InvalidStockAlias,
    /// Banding on a material that does not take it.
    BandingNotAccepted(Uuid),
}

impl Project {
    /// Lookups by identity. After `validate`, every reference held by another
    /// record resolves; callers holding an ID from UI state must still handle
    /// `None`, because that ID may be stale.
    pub fn board(&self, id: Uuid) -> Option<&Board> {
        self.boards.iter().find(|board| board.id == id)
    }

    pub fn board_mut(&mut self, id: Uuid) -> Option<&mut Board> {
        self.boards.iter_mut().find(|board| board.id == id)
    }

    pub fn material(&self, id: Uuid) -> Option<&Material> {
        self.materials.iter().find(|material| material.id == id)
    }

    pub fn edge_band(&self, id: Uuid) -> Option<&EdgeBand> {
        self.edge_bands.iter().find(|band| band.id == id)
    }

    pub fn stock_piece(&self, id: Uuid) -> Option<&Stock> {
        self.stock.iter().find(|piece| piece.id == id)
    }

    pub fn stock_piece_mut(&mut self, id: Uuid) -> Option<&mut Stock> {
        self.stock.iter_mut().find(|piece| piece.id == id)
    }

    pub fn stock_alias(&self, id: Uuid) -> Option<&str> {
        self.stock_aliases.get(&id).map(String::as_str)
    }

    /// Fill aliases missing from older documents in their original global order.
    /// This is an in-memory compatibility operation, not an edit or renumbering.
    pub(crate) fn assign_missing_stock_aliases(&mut self) -> Result<(), DomainError> {
        let missing: Vec<_> = self
            .ordered_stock()
            .into_iter()
            .filter(|piece| !self.stock_aliases.contains_key(&piece.id))
            .map(|piece| (piece.id, piece.source))
            .collect();
        for (id, source) in missing {
            self.assign_stock_alias(id, source)?;
        }
        Ok(())
    }

    pub(crate) fn assign_stock_alias(
        &mut self,
        id: Uuid,
        initial_source: StockSource,
    ) -> Result<(), DomainError> {
        if self.stock_aliases.contains_key(&id) {
            return Err(DomainError::InvalidStockAlias);
        }
        let (prefix, next) = match initial_source {
            StockSource::ToPurchase => ('S', &mut self.next_stock_s_alias),
            StockSource::Owned => ('O', &mut self.next_stock_o_alias),
        };
        let following = next.checked_add(1).ok_or(DomainError::InvalidStockAlias)?;
        self.stock_aliases.insert(id, format!("{prefix}{next}"));
        *next = following;
        Ok(())
    }

    /// Declared physical-piece order. Legacy equal priorities have a stable ID tie-break.
    pub fn ordered_stock(&self) -> Vec<&Stock> {
        let mut pieces: Vec<_> = self.stock.iter().collect();
        pieces.sort_by_key(|piece| (piece.priority, piece.id));
        pieces
    }

    pub fn new(name: impl Into<String>, currency: Currency) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            revision: 0,
            display_unit: Unit::Mm,
            grid_spacing: DEFAULT_GRID_SPACING,
            cutting_kerf: DEFAULT_CUTTING_KERF,
            confirmed_shop_kerf: None,
            confirmed_shop_kerf_unix_ms: None,
            currency,
            cut_fee: None,
            materials: Vec::new(),
            material_colors: BTreeMap::new(),
            edge_bands: Vec::new(),
            boards: Vec::new(),
            assemblies: Vec::new(),
            stock: Vec::new(),
            stock_aliases: BTreeMap::new(),
            next_stock_s_alias: 1,
            next_stock_o_alias: 1,
            allocations: Vec::new(),
            catalog: Vec::new(),
            hardware: Vec::new(),
            hinge_installations: Vec::new(),
            door_joints: Vec::new(),
            slide_installations: Vec::new(),
            export_records: Vec::new(),
            file_exports: Vec::new(),
        }
    }

    /// Structural validation of an entire candidate document before it replaces a project.
    /// Placement feasibility and hardware installation checks belong to later planners.
    pub fn validate(&self) -> Result<(), DomainError> {
        self.validate_version(SCHEMA_VERSION)
    }

    /// Local box of a hardware item: a placeholder's dimensions or a pinned
    /// foot's bounding box. `None` for catalog items without geometry.
    pub fn hardware_dimensions(&self, item: &Hardware) -> Option<[Length; 3]> {
        match &item.kind {
            HardwareKind::Placeholder { dimensions } => Some(*dimensions),
            HardwareKind::Catalog { catalog_id } => self
                .catalog
                .iter()
                .find(|entry| entry.id == *catalog_id)
                .and_then(CatalogReference::foot)
                .filter(|spec| spec.is_consistent())
                .map(FootSpec::local_size),
        }
    }

    /// The pinned foot facts of a hardware item, when it is a foot.
    pub fn foot_spec(&self, item: &Hardware) -> Option<&FootSpec> {
        match &item.kind {
            HardwareKind::Catalog { catalog_id } => self
                .catalog
                .iter()
                .find(|entry| entry.id == *catalog_id)
                .and_then(CatalogReference::foot),
            HardwareKind::Placeholder { .. } => None,
        }
    }

    pub fn material_color(&self, material_id: Uuid) -> SrgbColor {
        self.material_colors
            .get(&material_id)
            .copied()
            .unwrap_or(NEUTRAL_MATERIAL_COLOR)
    }

    /// The persistence compatibility path validates legacy data before changing
    /// its version. Normal editors and writers accept only the current schema.
    pub(crate) fn validate_version(&self, version: u32) -> Result<(), DomainError> {
        if self.schema_version != version {
            return Err(DomainError::UnsupportedVersion(self.schema_version));
        }
        validate_grid_spacing(self.grid_spacing).map_err(DomainError::InvalidGridSpacing)?;
        self.cutting_kerf
            .positive()
            .map_err(DomainError::InvalidCuttingKerf)?;
        if self
            .confirmed_shop_kerf
            .is_some_and(|kerf| kerf != self.cutting_kerf)
        {
            return Err(DomainError::InvalidCuttingKerf(UnitError::InvalidNumber));
        }
        // Unix milliseconds must represent a real UTC date within the four-digit
        // calendar range. In particular an orphan date cannot confer confirmation.
        if let Some(date) = self.confirmed_shop_kerf_unix_ms
            && (self.confirmed_shop_kerf.is_none() || date > 253_402_300_799_999)
        {
            return Err(DomainError::InvalidKerfConfirmationDate);
        }
        if self
            .cut_fee
            .is_some_and(|fee| fee.currency() != self.currency || fee.minor_units() < 0)
        {
            return Err(DomainError::InvalidCutFee);
        }
        let mut ids = HashSet::from([self.id]);
        for id in self
            .materials
            .iter()
            .map(|v| v.id)
            .chain(self.boards.iter().map(|v| v.id))
            .chain(self.assemblies.iter().map(|v| v.id))
            .chain(self.stock.iter().map(|v| v.id))
            .chain(self.allocations.iter().map(|v| v.id))
            .chain(self.catalog.iter().map(|v| v.id))
            .chain(self.hardware.iter().map(|v| v.id))
            .chain(self.hinge_installations.iter().map(|v| v.id))
            .chain(self.door_joints.iter().map(|v| v.id))
            .chain(self.slide_installations.iter().map(|v| v.id))
        {
            if !ids.insert(id) {
                return Err(DomainError::DuplicateId(id));
            }
        }
        let materials: HashSet<_> = self.materials.iter().map(|v| v.id).collect();
        for &material_id in self.material_colors.keys() {
            if !materials.contains(&material_id) {
                return Err(DomainError::DanglingReference {
                    owner: self.id,
                    target: material_id,
                });
            }
        }
        let boards: HashSet<_> = self.boards.iter().map(|v| v.id).collect();
        let stock: HashSet<_> = self.stock.iter().map(|v| v.id).collect();
        if version == SCHEMA_VERSION {
            if self.next_stock_s_alias == 0 || self.next_stock_o_alias == 0 {
                return Err(DomainError::InvalidStockAlias);
            }
            let mut aliases = HashSet::new();
            for (&id, alias) in &self.stock_aliases {
                let (number, next) = if let Some(number) = alias.strip_prefix('S') {
                    (number, self.next_stock_s_alias)
                } else if let Some(number) = alias.strip_prefix('O') {
                    (number, self.next_stock_o_alias)
                } else {
                    return Err(DomainError::InvalidStockAlias);
                };
                let valid_number = number
                    .parse::<u64>()
                    .ok()
                    .filter(|n| *n > 0 && n.to_string() == number && *n < next);
                if !stock.contains(&id) || valid_number.is_none() || !aliases.insert(alias) {
                    return Err(DomainError::InvalidStockAlias);
                }
            }
            if !self.stock_aliases.is_empty() && self.stock_aliases.len() != self.stock.len() {
                return Err(DomainError::InvalidStockAlias);
            }
        }
        let catalog: HashSet<_> = self.catalog.iter().map(|v| v.id).collect();
        let parents: HashMap<_, _> = self
            .assemblies
            .iter()
            .map(|v| (v.id, v.parent_id))
            .collect();
        let reference = |owner, target, set: &HashSet<Uuid>| {
            if set.contains(&target) {
                Ok(())
            } else {
                Err(DomainError::DanglingReference { owner, target })
            }
        };
        let check_pose = |owner, pose: Pose| {
            let normalized = Pose::new(pose.translation_mm, pose.rotation)
                .map_err(|reason| DomainError::InvalidPose { owner, reason })?;
            let q = pose.rotation;
            let norm = q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z;
            if !norm.is_finite()
                || (norm - 1.0).abs() > 1e-12
                || [q.w, q.x, q.y, q.z]
                    .iter()
                    .zip([
                        normalized.rotation.w,
                        normalized.rotation.x,
                        normalized.rotation.y,
                        normalized.rotation.z,
                    ])
                    .any(|(a, b)| (a - b).abs() > 1e-12)
            {
                return Err(DomainError::InvalidPose {
                    owner,
                    reason: UnitError::InvalidRotation,
                });
            }
            Ok(())
        };
        let bands: HashSet<_> = self.edge_bands.iter().map(|v| v.id).collect();
        for band in &self.edge_bands {
            positive(band.id, &[band.thickness, band.height])?;
        }
        for material in &self.materials {
            positive(material.id, &[material.default_thickness])?;
            if let Some(band) = material.default_band {
                reference(material.id, band, &bands)?;
            }
        }
        for board in &self.boards {
            reference(board.id, board.material_id, &materials)?;
            for band in board.banding.bands() {
                reference(board.id, band, &bands)?;
            }
            if board.banding.bands().next().is_some()
                && self
                    .material(board.material_id)
                    .is_some_and(|m| !m.kind.accepts_banding())
            {
                return Err(DomainError::BandingNotAccepted(board.id));
            }
            check_parent(board.id, board.parent_id, &parents)?;
            positive(board.id, &board.blank_dimensions())?;
            check_pose(board.id, board.pose)?;
        }
        for assembly in &self.assemblies {
            check_parent(assembly.id, assembly.parent_id, &parents)?;
            check_pose(assembly.id, assembly.pose)?;
            let mut seen = HashSet::new();
            let mut current = Some(assembly.id);
            while let Some(id) = current {
                if !seen.insert(id) {
                    return Err(DomainError::HierarchyCycle(id));
                }
                current = parents[&id];
            }
        }
        for piece in &self.stock {
            reference(piece.id, piece.material_id, &materials)?;
            positive(piece.id, &[piece.length, piece.width, piece.thickness])?;
            if piece.trim.iter().any(|v| v.micrometres() < 0) {
                return Err(DomainError::InvalidDimension(piece.id));
            }
            let [left, right, bottom, top] = piece.trim.map(|v| i128::from(v.micrometres()));
            if left + right >= i128::from(piece.length.micrometres())
                || bottom + top >= i128::from(piece.width.micrometres())
            {
                return Err(DomainError::InvalidDimension(piece.id));
            }
            if piece
                .price
                .is_some_and(|price| price.currency() != self.currency || price.minor_units() < 0)
            {
                return Err(DomainError::InvalidPrice(piece.id));
            }
        }
        for entry in &self.catalog {
            if entry
                .installation_dimensions
                .values()
                .any(|v| v.micrometres() < 0)
            {
                return Err(DomainError::InvalidDimension(entry.id));
            }
            if entry.verified_hinge.is_some() && !crate::hardware_catalog::is_verified(entry) {
                return Err(DomainError::InvalidCatalog(entry.id));
            }
            let coherent_item = match &entry.item {
                None => true,
                Some(CatalogItem::Slide(spec)) => spec.is_consistent(),
                Some(CatalogItem::Foot(spec)) => spec.is_consistent(),
            };
            if !coherent_item
                || (entry.item.is_some()
                    && (entry.verified_hinge.is_some()
                        || entry.plate_id.is_some()
                        || !entry.installation_dimensions.is_empty()))
            {
                return Err(DomainError::InvalidCatalog(entry.id));
            }
        }
        for item in &self.hardware {
            check_parent(item.id, item.parent_id, &parents)?;
            check_pose(item.id, item.pose)?;
            match &item.kind {
                HardwareKind::Placeholder { dimensions } => positive(item.id, dimensions)?,
                HardwareKind::Catalog { catalog_id } => {
                    reference(item.id, *catalog_id, &catalog)?;
                    // Slides are installations between boards, never loose objects.
                    if self
                        .catalog
                        .iter()
                        .any(|entry| entry.id == *catalog_id && entry.slide().is_some())
                    {
                        return Err(DomainError::InvalidCatalog(*catalog_id));
                    }
                }
            }
        }
        for installation in &self.hinge_installations {
            reference(installation.id, installation.door_board_id, &boards)?;
            reference(installation.id, installation.mounting_board_id, &boards)?;
            reference(installation.id, installation.catalog_id, &catalog)?;
            if installation.door_board_id == installation.mounting_board_id {
                return Err(DomainError::SameHingeBoards(installation.id));
            }
            // Out-of-bounds annotations and unsupported catalog parameters are
            // draft diagnostics, but negative physical distances are malformed.
            if [
                installation.door_y,
                installation.mount_y,
                installation.cup_edge_setback,
                installation.overlay,
            ]
            .iter()
            .any(|v| v.micrometres() < 0)
            {
                return Err(DomainError::InvalidDimension(installation.id));
            }
        }
        crate::door_joint::validate_joints(self)?;
        crate::slide_installation::validate_installations(self)?;
        let mut assigned = HashSet::new();
        for allocation in &self.allocations {
            reference(allocation.id, allocation.board_id, &boards)?;
            reference(allocation.id, allocation.stock_id, &stock)?;
            if !assigned.insert(allocation.board_id) {
                return Err(DomainError::DuplicateAllocation(allocation.board_id));
            }
            if allocation.origin.iter().any(|v| v.micrometres() < 0) {
                return Err(DomainError::InvalidDimension(allocation.id));
            }
        }
        for record in &self.export_records {
            if record.project_id != self.id || !record.is_valid() {
                return Err(DomainError::InvalidExportRecord);
            }
        }
        for record in &self.file_exports {
            if record.project_id != self.id || !record.is_valid() {
                return Err(DomainError::InvalidExportRecord);
            }
        }
        Ok(())
    }
}

fn positive(owner: Uuid, lengths: &[Length]) -> Result<(), DomainError> {
    if lengths.iter().any(|v| v.micrometres() <= 0) {
        Err(DomainError::InvalidDimension(owner))
    } else {
        Ok(())
    }
}

fn check_parent(
    owner: Uuid,
    parent: Option<Uuid>,
    parents: &HashMap<Uuid, Option<Uuid>>,
) -> Result<(), DomainError> {
    if let Some(target) = parent
        && !parents.contains_key(&target)
    {
        return Err(DomainError::DanglingReference { owner, target });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Quaternion;

    fn mm(value: i64) -> Length {
        Length::from_micrometres(value * 1000)
    }
    fn pose() -> Pose {
        Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap()
    }
    fn fixture() -> Project {
        let mut project = Project::new("Cabinet", Currency::Brl);
        let material = Material {
            coating: Default::default(),
            default_band: None,
            kind: Default::default(),
            id: Uuid::new_v4(),
            name: "Plywood".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Length,
        };
        let board = Board {
            coated_face: Default::default(),
            banding: Default::default(),
            id: Uuid::new_v4(),
            name: "Side".into(),
            material_id: material.id,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: pose(),
        };
        let stock = Stock {
            id: Uuid::new_v4(),
            name: "Sheet".into(),
            material_id: material.id,
            length: mm(200),
            width: mm(100),
            thickness: mm(18),
            grain: StockGrain::AlongX,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        };
        project.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: board.id,
            stock_id: stock.id,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: false,
        });
        project.materials.push(material);
        project.boards.push(board);
        project.stock.push(stock);
        project
    }

    #[test]
    fn same_labels_keep_independent_ids_and_allocations() {
        let mut p = fixture();
        let copy = p.boards[0].duplicate();
        assert_eq!(copy.name, p.boards[0].name);
        assert_ne!(copy.id, p.boards[0].id);
        p.boards.push(copy.clone());
        let other = p.stock[0].duplicate();
        assert_ne!(other.id, p.stock[0].id);
        p.stock.push(other.clone());
        p.allocations.push(Allocation {
            id: Uuid::new_v4(),
            board_id: copy.id,
            stock_id: other.id,
            origin: [Length::ZERO; 2],
            quarter_turn: false,
            locked: false,
        });
        assert_eq!(p.validate(), Ok(()));
        assert_eq!(p.allocations[0].board_id, p.boards[0].id);
    }

    #[test]
    fn detects_dangling_references_and_cross_type_id_collisions() {
        let mut p = fixture();
        let unknown = Uuid::new_v4();
        p.boards[0].material_id = unknown;
        assert_eq!(
            p.validate(),
            Err(DomainError::DanglingReference {
                owner: p.boards[0].id,
                target: unknown
            })
        );
        p.boards[0].material_id = p.materials[0].id;
        p.allocations[0].stock_id = unknown;
        assert_eq!(
            p.validate(),
            Err(DomainError::DanglingReference {
                owner: p.allocations[0].id,
                target: unknown
            })
        );
        p.allocations[0].stock_id = p.stock[0].id;
        p.hardware.push(Hardware {
            id: Uuid::new_v4(),
            name: "Hinge".into(),
            parent_id: None,
            pose: pose(),
            kind: HardwareKind::Catalog {
                catalog_id: unknown,
            },
        });
        assert_eq!(
            p.validate(),
            Err(DomainError::DanglingReference {
                owner: p.hardware[0].id,
                target: unknown
            })
        );
        p.hardware.clear();
        p.stock[0].id = p.materials[0].id;
        assert_eq!(
            p.validate(),
            Err(DomainError::DuplicateId(p.materials[0].id))
        );
    }

    #[test]
    fn detects_cycles_and_invalid_parents() {
        let mut p = fixture();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        p.assemblies = vec![
            Assembly {
                id: a,
                name: "A".into(),
                parent_id: Some(b),
                pose: pose(),
            },
            Assembly {
                id: b,
                name: "B".into(),
                parent_id: Some(a),
                pose: pose(),
            },
        ];
        assert!(matches!(p.validate(), Err(DomainError::HierarchyCycle(_))));
        p.assemblies[1].parent_id = None;
        p.boards[0].parent_id = Some(Uuid::new_v4());
        assert!(matches!(
            p.validate(),
            Err(DomainError::DanglingReference { .. })
        ));
    }

    #[test]
    fn rejects_nonfinite_unnormalized_pose_dimensions_and_duplicate_assignments() {
        let mut p = fixture();
        p.boards[0].pose.translation_mm[0] = f64::NAN;
        assert!(matches!(
            p.validate(),
            Err(DomainError::InvalidPose {
                reason: UnitError::NonFinite,
                ..
            })
        ));
        p.boards[0].pose = pose();
        p.boards[0].pose.rotation.w = 2.0;
        assert!(matches!(
            p.validate(),
            Err(DomainError::InvalidPose {
                reason: UnitError::InvalidRotation,
                ..
            })
        ));
        p.boards[0].pose = pose();
        p.boards[0].width = Length::ZERO;
        assert_eq!(
            p.validate(),
            Err(DomainError::InvalidDimension(p.boards[0].id))
        );
        p.boards[0].width = mm(50);
        let mut allocation = p.allocations[0].clone();
        allocation.id = Uuid::new_v4();
        p.allocations.push(allocation);
        assert_eq!(
            p.validate(),
            Err(DomainError::DuplicateAllocation(p.boards[0].id))
        );
    }

    #[test]
    fn refuses_unsupported_versions() {
        let mut p = fixture();
        for version in [0, 1, SCHEMA_VERSION + 1] {
            p.schema_version = version;
            assert_eq!(p.validate(), Err(DomainError::UnsupportedVersion(version)));
        }
    }
}
