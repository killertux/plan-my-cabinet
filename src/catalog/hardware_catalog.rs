//! Hardware records pinned into projects (hinges, drawer slides, feet), and
//! how far their facts can be trusted.
//!
//! Records come from catalog packs (`catalog_pack`). A project keeps its own
//! copy, so a changed or missing pack never alters a saved project. Trust is
//! derived, never stored: a record is `Reviewed` only while it equals a
//! record of a reviewed pack compiled into this application.
use std::sync::OnceLock;

use uuid::Uuid;

use crate::catalog_pack::{CatalogRegistry, ReviewStatus, snapshot, snapshot_foot, snapshot_slide};
use crate::commands::{EditError, ProjectEditor};
use crate::domain::{CatalogItem, CatalogReference, FootSpec, Project, SlideSpec, VerifiedHinge};
use crate::hinge_installation::{InstallationStatus, diagnose};
use crate::units::Length;

/// The first reviewed kit, kept as named anchors for its source review
/// (`docs/hinge-source-review.md`) and for tests.
pub const KIT_ID: &str = "51MX153DRV00100";
pub const PLATE_ID: &str = "52MX15FG11003D";
pub const SOURCE_URL: &str = "https://www.fgvtn.com.br/site/novopdf/Catalogo_Geral.pdf";
pub const SOURCE_SHA256: &str = "e8aafa4f3656a108e8e91dd1681685455a4bf4f644cf01d4ecfa3fd80bf44df2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    /// Equals a record of a reviewed pack bundled with the application.
    Reviewed,
    /// Coherent facts from a user pack, an older bundled revision or a draft.
    /// Guidance is given and labelled as user-supplied data.
    UserSupplied,
    /// Equals a record of a bundled pack of representative dimensions of a
    /// common product type (not a manufacturer sheet).
    Generic,
}

/// Default family for drawer slides: full extension, 45 kg, 350–550 mm.
pub const DEFAULT_SLIDE_FAMILY: (&str, &str) = ("fgvtn-slides", "tt45-slowmotion");

fn bundled() -> &'static CatalogRegistry {
    static REGISTRY: OnceLock<CatalogRegistry> = OnceLock::new();
    REGISTRY.get_or_init(CatalogRegistry::bundled)
}

/// Every record of the bundled reviewed packs, as snapshots.
fn reviewed_records() -> &'static [CatalogReference] {
    bundled_records(ReviewStatus::Reviewed)
}

fn bundled_records(status: ReviewStatus) -> &'static [CatalogReference] {
    static REVIEWED: OnceLock<Vec<CatalogReference>> = OnceLock::new();
    static GENERIC: OnceLock<Vec<CatalogReference>> = OnceLock::new();
    let cell = match status {
        ReviewStatus::Generic => &GENERIC,
        _ => &REVIEWED,
    };
    cell.get_or_init(|| {
        let mut records = Vec::new();
        for pack in bundled().usable() {
            if pack.review.status != status {
                continue;
            }
            for family in &pack.hinges {
                for variant in &family.variants {
                    records.push(snapshot(pack, family, variant, "en"));
                }
            }
            for language in ["en", "pt-BR"] {
                for family in &pack.slides {
                    for variant in &family.variants {
                        records.push(snapshot_slide(pack, family, variant, language));
                    }
                }
                for family in &pack.feet {
                    for variant in &family.variants {
                        records.push(snapshot_foot(pack, family, variant, language));
                    }
                }
            }
        }
        records
    })
}

/// Pinned slide facts usable for guidance.
pub fn slide_spec(entry: &CatalogReference) -> Option<&SlideSpec> {
    entry.slide().filter(|s| s.is_consistent())
}

/// Pinned foot facts usable for drawing and lists.
pub fn foot_spec(entry: &CatalogReference) -> Option<&FootSpec> {
    entry.foot().filter(|s| s.is_consistent())
}

/// The record gives guidance: coherent hinge, slide or foot facts.
pub fn is_usable(entry: &CatalogReference) -> bool {
    match &entry.item {
        None => is_verified(entry),
        Some(CatalogItem::Slide(s)) => s.is_consistent(),
        Some(CatalogItem::Foot(f)) => f.is_consistent(),
    }
}

fn same_facts(a: &CatalogReference, b: &CatalogReference) -> bool {
    a.product_id == b.product_id
        && a.plate_id == b.plate_id
        && a.source == b.source
        && a.revision == b.revision
        && a.installation_dimensions.is_empty()
        && a.verified_hinge == b.verified_hinge
        && same_item(a.item.as_ref(), b.item.as_ref())
}

/// Items compare without the display family name, which is localized.
fn same_item(a: Option<&CatalogItem>, b: Option<&CatalogItem>) -> bool {
    match (a, b) {
        (Some(CatalogItem::Slide(a)), Some(CatalogItem::Slide(b))) => {
            SlideSpec {
                family: String::new(),
                ..a.clone()
            } == SlideSpec {
                family: String::new(),
                ..b.clone()
            }
        }
        (a, b) => a == b,
    }
}

/// Pinned facts usable for numeric guidance: present and internally
/// consistent. Legacy free-form dimension maps never qualify.
pub fn facts(entry: &CatalogReference) -> Option<&VerifiedHinge> {
    entry.verified_hinge.as_ref().filter(|facts| {
        entry.installation_dimensions.is_empty() && crate::catalog_pack::facts_are_consistent(facts)
    })
}

pub fn is_verified(entry: &CatalogReference) -> bool {
    facts(entry).is_some()
}

/// `None` when the record gives no guidance at all.
pub fn trust(entry: &CatalogReference) -> Option<Trust> {
    if entry.item.is_some() {
        if !is_usable(entry) {
            return None;
        }
        let matches = |records: &[CatalogReference]| records.iter().any(|r| same_facts(entry, r));
        return Some(if matches(reviewed_records()) {
            Trust::Reviewed
        } else if matches(bundled_records(ReviewStatus::Generic)) {
            Trust::Generic
        } else {
            Trust::UserSupplied
        });
    }
    facts(entry)?;
    Some(
        if reviewed_records()
            .iter()
            .any(|record| same_facts(entry, record))
        {
            Trust::Reviewed
        } else {
            Trust::UserSupplied
        },
    )
}

/// The first reviewed kit (Click 3D Slow Reta / Calço 0) with a fresh ID.
pub fn builtin_hinge() -> CatalogReference {
    let mut entry = reviewed_records()
        .iter()
        .find(|r| r.product_id == KIT_ID)
        .cloned()
        .expect("bundled FGVTN pack lists the reviewed kit");
    entry.id = Uuid::new_v4();
    entry
}

pub fn supported_setting(entry: &CatalogReference, setback: Length, overlay: Length) -> bool {
    facts(entry).is_some_and(|facts| {
        facts
            .overlay_by_cup_edge
            .iter()
            .any(|setting| setting.cup_edge_setback == setback && setting.overlay == overlay)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogEditError {
    MissingEntry,
    /// No loaded pack has a record to replace this one with.
    NotInCatalog,
    DependentInstallation(String),
}

/// Add a pack record as an independent project-owned snapshot.
pub fn add(
    editor: &mut ProjectEditor,
    mut entry: CatalogReference,
) -> Result<Uuid, EditError<CatalogEditError>> {
    entry.id = Uuid::new_v4();
    let id = entry.id;
    editor.transact(|project| {
        project.catalog.push(entry);
        Ok(())
    })?;
    Ok(id)
}

pub fn add_builtin(editor: &mut ProjectEditor) -> Result<Uuid, EditError<CatalogEditError>> {
    add(editor, builtin_hinge())
}

/// The record `registry` would replace `entry` with, if it differs. Records
/// from before catalog packs are matched to a bundled one by product code.
pub fn replacement(
    registry: &CatalogRegistry,
    entry: &CatalogReference,
    language: &str,
) -> Option<CatalogReference> {
    let mut fresh = match (&entry.origin, &entry.item) {
        (Some(origin), Some(CatalogItem::Slide(_))) => {
            let (pack, family, variant) = registry.find_slide(origin)?;
            snapshot_slide(pack, family, variant, language)
        }
        (Some(origin), Some(CatalogItem::Foot(_))) => {
            let (pack, family, variant) = registry.find_foot(origin)?;
            snapshot_foot(pack, family, variant, language)
        }
        (None, Some(_)) => return None,
        (Some(origin), None) => {
            let (pack, family, variant) = registry.find_variant(origin)?;
            snapshot(pack, family, variant, language)
        }
        (None, None) => registry.usable().into_iter().find_map(|pack| {
            pack.hinges.iter().find_map(|family| {
                family
                    .variants
                    .iter()
                    .find(|v| v.code == entry.product_id)
                    .map(|variant| snapshot(pack, family, variant, language))
            })
        })?,
    };
    fresh.id = entry.id;
    // Keep the pinned name language: only facts are compared for updates.
    if fresh.item.is_some() {
        fresh.name.clone_from(&entry.name);
        if let (Some(CatalogItem::Slide(new)), Some(CatalogItem::Slide(old))) =
            (&mut fresh.item, &entry.item)
        {
            new.family.clone_from(&old.family);
        }
    }
    Some(fresh)
}

/// A newer pack record exists for this pinned entry.
pub fn update_available(registry: &CatalogRegistry, entry: &CatalogReference) -> bool {
    replacement(registry, entry, "en")
        .is_some_and(|fresh| !same_facts(entry, &fresh) || fresh.origin != entry.origin)
}

/// Explicit replacement, retaining the project-local ID and recomputing every
/// dependent installation's diagnostics in the same candidate transaction.
pub fn update_from_catalog_with_status(
    editor: &mut ProjectEditor,
    registry: &CatalogRegistry,
    catalog_id: Uuid,
    language: &str,
) -> Result<(bool, Vec<InstallationStatus>), EditError<CatalogEditError>> {
    let mut statuses = Vec::new();
    let changed = update_from_catalog(editor, registry, catalog_id, language, |candidate, _| {
        statuses = candidate
            .hinge_installations
            .iter()
            .filter(|i| i.catalog_id == catalog_id)
            .map(|i| diagnose(candidate, i))
            .collect();
        Ok(())
    })?;
    Ok((changed, statuses))
}

/// Compatibility entry point: replace from the bundled packs.
pub fn update_from_builtin(
    editor: &mut ProjectEditor,
    catalog_id: Uuid,
    validate_installations: impl FnMut(&Project, &[Uuid]) -> Result<(), String>,
) -> Result<bool, EditError<CatalogEditError>> {
    update_from_catalog(editor, bundled(), catalog_id, "en", validate_installations)
}

pub fn update_from_builtin_with_status(
    editor: &mut ProjectEditor,
    catalog_id: Uuid,
) -> Result<(bool, Vec<InstallationStatus>), EditError<CatalogEditError>> {
    update_from_catalog_with_status(editor, bundled(), catalog_id, "en")
}

/// Installation diagnosis always runs before the hook, regardless of what
/// the caller does with it.
pub fn update_from_catalog(
    editor: &mut ProjectEditor,
    registry: &CatalogRegistry,
    catalog_id: Uuid,
    language: &str,
    mut validate_installations: impl FnMut(&Project, &[Uuid]) -> Result<(), String>,
) -> Result<bool, EditError<CatalogEditError>> {
    let current = editor
        .project()
        .catalog
        .iter()
        .find(|entry| entry.id == catalog_id)
        .ok_or(EditError::Command(CatalogEditError::MissingEntry))?;
    let replacement = replacement(registry, current, language)
        .filter(is_usable)
        .ok_or(EditError::Command(CatalogEditError::NotInCatalog))?;
    editor.transact(|candidate| {
        let entry = candidate
            .catalog
            .iter_mut()
            .find(|entry| entry.id == catalog_id)
            .ok_or(CatalogEditError::MissingEntry)?;
        *entry = replacement;
        let dependent_ids: Vec<_> = candidate
            .hardware
            .iter()
            .filter_map(|hardware| match hardware.kind {
                crate::domain::HardwareKind::Catalog { catalog_id: id } if id == catalog_id => {
                    Some(hardware.id)
                }
                _ => None,
            })
            .collect();
        let _statuses: Vec<_> = candidate
            .hinge_installations
            .iter()
            .filter(|i| i.catalog_id == catalog_id)
            .map(|i| diagnose(candidate, i))
            .collect();
        validate_installations(candidate, &dependent_ids)
            .map_err(CatalogEditError::DependentInstallation)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CatalogReference, DomainError, Hardware, HardwareKind, UnavailableDetail};
    use crate::money::Currency;
    use crate::persistence::{prepare_bytes, save};
    use crate::units::{Pose, Quaternion};

    fn mm(value: i64) -> Length {
        Length::from_micrometres(value * 1_000)
    }

    #[test]
    fn source_fixture_and_supported_geometry_are_exact() {
        let review = include_str!("../../docs/hinge-source-review.md");
        for evidence in [
            KIT_ID,
            PLATE_ID,
            SOURCE_URL,
            SOURCE_SHA256,
            "printed page 23 (PDF page 14)",
            "15 a 22 mm",
            "Ø35 × 11,3 mm",
            "32 mm",
            "37 mm",
            "105°",
        ] {
            assert!(
                review.contains(evidence),
                "missing source evidence: {evidence}"
            );
        }
        let entry = builtin_hinge();
        assert!(is_verified(&entry));
        let facts = entry.verified_hinge.as_ref().unwrap();
        assert_eq!(facts.plate_height, Length::ZERO);
        assert_eq!((facts.printed_page, facts.pdf_page), (23, 14));
        assert_eq!(facts.overlay_by_cup_edge.len(), 4);
        for k in 3..=6 {
            assert!(supported_setting(&entry, mm(k), mm(k + 12)));
            assert!(!supported_setting(&entry, mm(k), mm(k + 13)));
        }
        assert_eq!(facts.cup_depth.micrometres(), 11_300);
    }

    #[test]
    fn bilingual_guides_and_export_disclosures_match_reviewed_record() {
        use crate::i18n::{Language, Localizer};
        let entry = builtin_hinge();
        let facts = entry.verified_hinge.as_ref().unwrap();
        let guides = [
            include_str!("../../docs/hardware-en.md"),
            include_str!("../../docs/hardware-pt-BR.md"),
        ];
        for (guide, language) in guides.into_iter().zip([Language::En, Language::PtBr]) {
            for expected in [
                entry.product_id.as_str(),
                entry.plate_id.as_deref().unwrap(),
                facts.source_sha256.as_str(),
                "3/15, 4/16, 5/17, 6/18",
                "15–22 mm",
                "32 mm",
                "37 mm",
                "105°",
            ] {
                assert!(guide.contains(expected), "guide missing {expected}");
            }
            let localizer = Localizer::new(language);
            for key in [
                "hinge-fasteners-unavailable",
                "door-motion-disclosure",
                "pdf-fasteners-unavailable",
                "pdf-hinge-review-warning",
                "pdf-motion-approximate",
                "pdf-face-min-z",
            ] {
                assert_ne!(localizer.text(key), key, "untranslated {key}");
            }
        }
        assert_eq!(facts.opening_limit_degrees, 105);
        assert_eq!(facts.plate_hole_pitch, mm(32));
        assert_eq!(facts.plate_front_offset, mm(37));
        assert_eq!(facts.screw_details, UnavailableDetail::Unavailable);
    }

    #[test]
    fn modified_facts_lose_reviewed_trust_and_incoherent_facts_are_rejected() {
        let baseline = builtin_hinge();
        assert_eq!(trust(&baseline), Some(Trust::Reviewed));
        // A renamed copy keeps its trust: the name is the user's label.
        let mut renamed = baseline.clone();
        renamed.name = "Kitchen hinge".into();
        assert_eq!(trust(&renamed), Some(Trust::Reviewed));
        for change in 0..4 {
            let mut entry = baseline.clone();
            match change {
                0 => entry.product_id = "51MX153DCV00100".into(),
                1 => entry.plate_id = None,
                2 => entry.verified_hinge.as_mut().unwrap().cup_depth = mm(12),
                _ => entry.verified_hinge.as_mut().unwrap().source_sha256 = "0".repeat(64),
            }
            // Coherent but different facts: still usable, labelled as user data.
            assert_eq!(trust(&entry), Some(Trust::UserSupplied), "change {change}");
            let mut project = Project::new("Edited", Currency::Brl);
            project.catalog.push(entry);
            assert_eq!(project.validate(), Ok(()));
        }
        for change in 0..3 {
            let mut entry = baseline.clone();
            let facts = entry.verified_hinge.as_mut().unwrap();
            match change {
                0 => facts.cup_depth = mm(15),
                1 => facts.overlay_by_cup_edge.reverse(),
                _ => {
                    entry
                        .installation_dimensions
                        .insert("pilot_depth".into(), mm(8));
                }
            }
            assert!(!is_verified(&entry));
            assert_eq!(trust(&entry), None);
            let mut project = Project::new("Incoherent", Currency::Brl);
            project.catalog.push(entry.clone());
            assert_eq!(
                project.validate(),
                Err(DomainError::InvalidCatalog(entry.id))
            );
        }
    }

    #[test]
    fn offline_reopen_and_explicit_update_are_atomic_undoable() {
        let mut editor = ProjectEditor::new(Project::new("Cabinet", Currency::Brl)).unwrap();
        let id = add_builtin(&mut editor).unwrap();
        editor
            .transact(|project| -> Result<(), ()> {
                project.hardware.push(Hardware {
                    id: Uuid::new_v4(),
                    name: "Hinge".into(),
                    parent_id: None,
                    pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
                    kind: HardwareKind::Catalog { catalog_id: id },
                });
                Ok(())
            })
            .unwrap();
        let path = std::env::temp_dir().join(format!("catalog-{}.pmcab", Uuid::new_v4()));
        save(&mut editor, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let mut reopened = prepare_bytes(&bytes).unwrap().into_editor();
        // Simulate a future packaged catalog label/revision without touching the saved data.
        let mut changed_builtin = builtin_hinge();
        changed_builtin.name = "Future package label".into();
        changed_builtin.revision = "Future packaged revision".into();
        assert_ne!(reopened.project().catalog[0].name, changed_builtin.name);
        assert_ne!(
            reopened.project().catalog[0].revision,
            changed_builtin.revision
        );
        assert_eq!(reopened.project().catalog[0], editor.project().catalog[0]);
        let original = reopened.project().catalog[0].clone();
        reopened
            .transact(|p| -> Result<(), ()> {
                p.catalog[0].name = "My pinned label".into();
                Ok(())
            })
            .unwrap();
        let before = reopened.project().clone();
        let hardware_id = before.hardware[0].id;
        assert_eq!(
            update_from_builtin(&mut reopened, id, |_, dependents| {
                assert_eq!(dependents, &[hardware_id]);
                Err("installation needs review".into())
            }),
            Err(EditError::Command(CatalogEditError::DependentInstallation(
                "installation needs review".into()
            )))
        );
        assert_eq!(reopened.project(), &before);
        assert!(
            update_from_builtin(&mut reopened, id, |candidate, dependents| {
                assert_eq!(dependents, &[hardware_id]);
                assert_eq!(candidate.catalog[0].id, id);
                Ok(())
            })
            .unwrap()
        );
        assert_eq!(reopened.project().catalog[0], original);
        assert_eq!(reopened.undo(), Ok(true));
        assert_eq!(
            reopened.project(),
            &Project {
                revision: reopened.project().revision,
                ..before
            }
        );
    }

    #[test]
    fn legacy_v1_reference_has_no_verified_claim() {
        let mut project = Project::new("Legacy", Currency::Brl);
        project.catalog.push(CatalogReference {
            verified_hinge: None,
            ..builtin_hinge()
        });
        let mut json = serde_json::to_value(project).unwrap();
        json["catalog"][0]
            .as_object_mut()
            .unwrap()
            .remove("verified_hinge");
        let reopened = prepare_bytes(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(reopened.project().catalog[0].verified_hinge.is_none());
        assert!(!is_verified(&reopened.project().catalog[0]));
    }
}
