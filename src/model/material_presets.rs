//! Standard sheet materials offered to every new project, and the swatch
//! palette used when choosing a material's display colour.
//!
//! Presets are a starting point, not a persisted type: seeding a project copies
//! them into ordinary `Material` records the user can rename or edit.
use uuid::Uuid;

use crate::domain::{BoardGrain, EdgeBand, Material, MaterialKind, Project, SrgbColor};
use crate::i18n::Language;
use crate::units::Length;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialPreset {
    pub name_en: &'static str,
    pub name_pt: &'static str,
    pub thickness_mm: i64,
    pub grain: BoardGrain,
    pub color: SrgbColor,
    /// Standard full-sheet size (length × width) sold in Brazil.
    pub sheet_mm: [i64; 2],
}

impl MaterialPreset {
    pub fn name(&self, language: Language) -> &'static str {
        match language {
            Language::En => self.name_en,
            Language::PtBr => self.name_pt,
        }
    }

    pub fn kind(&self) -> MaterialKind {
        MaterialKind::infer(self.name_en)
    }

    pub fn thickness(&self) -> Length {
        Length::from_micrometres(self.thickness_mm * 1000)
    }

    pub fn sheet_size(&self) -> [Length; 2] {
        self.sheet_mm.map(|mm| Length::from_micrometres(mm * 1000))
    }

    fn matches(&self, material: &Material) -> bool {
        material.default_thickness == self.thickness()
            && (material.name.eq_ignore_ascii_case(self.name_en)
                || material.name.eq_ignore_ascii_case(self.name_pt))
    }
}

const WHITE_MDF: SrgbColor = SrgbColor([244, 242, 238]);
const RAW_MDF: SrgbColor = SrgbColor([196, 166, 126]);
const WHITE_MDP: SrgbColor = SrgbColor([238, 236, 230]);
const PLYWOOD: SrgbColor = SrgbColor([222, 190, 140]);
const HDF: SrgbColor = SrgbColor([182, 154, 120]);

const fn preset(
    name_en: &'static str,
    name_pt: &'static str,
    thickness_mm: i64,
    grain: BoardGrain,
    color: SrgbColor,
    sheet_mm: [i64; 2],
) -> MaterialPreset {
    MaterialPreset {
        name_en,
        name_pt,
        thickness_mm,
        grain,
        color,
        sheet_mm,
    }
}

/// The Brazilian standard set seeded into new projects.
pub const BR_STANDARD: &[MaterialPreset] = &[
    preset(
        "White MDF",
        "MDF Branco",
        15,
        BoardGrain::Unrestricted,
        WHITE_MDF,
        [2750, 1840],
    ),
    preset(
        "White MDF",
        "MDF Branco",
        18,
        BoardGrain::Unrestricted,
        WHITE_MDF,
        [2750, 1840],
    ),
    preset(
        "Raw MDF",
        "MDF Cru",
        6,
        BoardGrain::Unrestricted,
        RAW_MDF,
        [2750, 1840],
    ),
    preset(
        "Raw MDF",
        "MDF Cru",
        15,
        BoardGrain::Unrestricted,
        RAW_MDF,
        [2750, 1840],
    ),
    preset(
        "Raw MDF",
        "MDF Cru",
        18,
        BoardGrain::Unrestricted,
        RAW_MDF,
        [2750, 1840],
    ),
    preset(
        "White MDP",
        "MDP Branco",
        15,
        BoardGrain::Unrestricted,
        WHITE_MDP,
        [2750, 1840],
    ),
    preset(
        "White MDP",
        "MDP Branco",
        18,
        BoardGrain::Unrestricted,
        WHITE_MDP,
        [2750, 1840],
    ),
    preset(
        "Plywood",
        "Compensado",
        6,
        BoardGrain::Length,
        PLYWOOD,
        [2200, 1600],
    ),
    preset(
        "Plywood",
        "Compensado",
        15,
        BoardGrain::Length,
        PLYWOOD,
        [2200, 1600],
    ),
    preset(
        "Plywood",
        "Compensado",
        18,
        BoardGrain::Length,
        PLYWOOD,
        [2200, 1600],
    ),
    preset("HDF", "HDF", 3, BoardGrain::Unrestricted, HDF, [2750, 1830]),
];

/// Named display colours for materials. `None` is not offered here; callers
/// add a "no colour" choice themselves.
pub const SWATCHES: &[(&str, SrgbColor)] = &[
    ("color-white", SrgbColor([244, 242, 238])),
    ("color-off-white", SrgbColor([234, 228, 214])),
    ("color-light-grey", SrgbColor([200, 200, 196])),
    ("color-graphite", SrgbColor([92, 92, 96])),
    ("color-black", SrgbColor([44, 42, 42])),
    ("color-raw-mdf", RAW_MDF),
    ("color-birch", SrgbColor([232, 212, 176])),
    ("color-oak", SrgbColor([214, 180, 132])),
    ("color-freijo", SrgbColor([184, 142, 92])),
    ("color-walnut", SrgbColor([124, 88, 60])),
    ("color-sage", SrgbColor([168, 184, 160])),
    ("color-navy", SrgbColor([52, 70, 104])),
];

/// A standard edge band roll, named as Brazilian shops list them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandPreset {
    pub name_en: &'static str,
    pub name_pt: &'static str,
    /// Thickness and height in hundredths of a millimetre.
    pub size_hundredths: [i64; 2],
    pub color: SrgbColor,
}

impl BandPreset {
    pub fn name(&self, language: Language) -> &'static str {
        match language {
            Language::En => self.name_en,
            Language::PtBr => self.name_pt,
        }
    }
}

/// Bands seeded into new projects. The first is the default band of the white
/// MDF and MDP presets.
pub const BR_BANDS: &[BandPreset] = &[
    BandPreset {
        name_en: "White band 1x22",
        name_pt: "Fita Branca 1x22",
        size_hundredths: [100, 2200],
        color: WHITE_MDF,
    },
    BandPreset {
        name_en: "White band 0.45x22",
        name_pt: "Fita Branca 0,45x22",
        size_hundredths: [45, 2200],
        color: WHITE_MDF,
    },
    BandPreset {
        name_en: "White band 1x35",
        name_pt: "Fita Branca 1x35",
        size_hundredths: [100, 3500],
        color: WHITE_MDF,
    },
    BandPreset {
        name_en: "Raw band 0.45x22",
        name_pt: "Fita Crua 0,45x22",
        size_hundredths: [45, 2200],
        color: RAW_MDF,
    },
];

/// Adds the standard set to a project that has no materials yet. Returns the
/// number of materials added (zero when the project already has some).
/// Standard bands are added alongside, and white sheets band in white.
pub fn seed_defaults(project: &mut Project, language: Language) -> usize {
    if !project.materials.is_empty() {
        return 0;
    }
    let mut white_band = None;
    if project.edge_bands.is_empty() {
        for preset in BR_BANDS {
            let id = Uuid::new_v4();
            white_band.get_or_insert(id);
            let [thickness, height] = preset
                .size_hundredths
                .map(|h| Length::from_micrometres(h * 10));
            project.edge_bands.push(EdgeBand {
                id,
                name: preset.name(language).to_owned(),
                thickness,
                height,
                color: preset.color,
            });
        }
    }
    for preset in BR_STANDARD {
        let id = Uuid::new_v4();
        let kind = preset.kind();
        let white = preset.color == WHITE_MDF || preset.color == WHITE_MDP;
        project.materials.push(Material {
            id,
            name: preset.name(language).to_owned(),
            coating: crate::domain::Coating::infer(preset.name(Language::En)),
            default_thickness: preset.thickness(),
            default_grain: preset.grain,
            kind,
            default_band: white_band.filter(|_| kind.accepts_banding() && white),
        });
        project.material_colors.insert(id, preset.color);
    }
    BR_STANDARD.len()
}

/// The preset a material was created from (or still matches by name and
/// thickness), used to pre-fill standard sheet sizes.
pub fn preset_for(material: &Material) -> Option<&'static MaterialPreset> {
    BR_STANDARD.iter().find(|preset| preset.matches(material))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;

    #[test]
    fn seeding_adds_valid_coloured_materials_once() {
        let mut project = Project::new("New", Currency::Brl);
        assert_eq!(
            seed_defaults(&mut project, Language::PtBr),
            BR_STANDARD.len()
        );
        project.validate().unwrap();
        assert_eq!(project.material_colors.len(), BR_STANDARD.len());
        assert!(project.materials.iter().any(|m| m.name == "Compensado"));
        assert_eq!(seed_defaults(&mut project, Language::PtBr), 0);
        for material in &project.materials {
            let preset = preset_for(material).expect("seeded material matches its preset");
            assert_eq!(preset.thickness(), material.default_thickness);
        }
    }

    #[test]
    fn english_names_match_the_same_presets() {
        let mut project = Project::new("New", Currency::Brl);
        seed_defaults(&mut project, Language::En);
        assert!(project.materials.iter().any(|m| m.name == "Plywood"));
        assert!(project.materials.iter().all(|m| preset_for(m).is_some()));
    }
}
