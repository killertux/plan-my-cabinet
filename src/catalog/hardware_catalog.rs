//! Offline factual catalog. No manufacturer artwork, PDF, or runtime requests.
//! Evidence: docs/hinge-source-review.md, visually inspected printed p. 23.

use std::collections::HashMap;

use uuid::Uuid;

use crate::commands::{EditError, ProjectEditor};
use crate::domain::{CatalogReference, OverlaySetting, Project, UnavailableDetail, VerifiedHinge};
use crate::hinge_installation::{InstallationStatus, diagnose};
use crate::units::Length;

pub const KIT_ID: &str = "51MX153DRV00100";
pub const PLATE_ID: &str = "52MX15FG11003D";
pub const SOURCE_URL: &str = "https://www.fgvtn.com.br/site/novopdf/Catalogo_Geral.pdf";
pub const SOURCE_SHA256: &str = "e8aafa4f3656a108e8e91dd1681685455a4bf4f644cf01d4ecfa3fd80bf44df2";

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1_000)
}

/// Frozen reviewed revision: keep this validator available when a future
/// packaged catalog selects a newer record, so old pinned projects still open.
fn reviewed_2025_hinge() -> CatalogReference {
    CatalogReference {
        id: Uuid::new_v4(),
        name: "FGVTN Click 3D Slow Reta / Calço 0 (complete kit)".into(),
        product_id: KIT_ID.into(),
        plate_id: Some(PLATE_ID.into()),
        source: SOURCE_URL.into(),
        revision: format!("May 2025 catalog; modified 2026-09-16; SHA-256 {SOURCE_SHA256}"),
        // The legacy dimension bag is deliberately empty. Typed facts have named
        // coordinate references; an arbitrary key must not become drilling advice.
        installation_dimensions: HashMap::new(),
        verified_hinge: Some(VerifiedHinge {
            printed_page: 23,
            pdf_page: 14,
            source_sha256: SOURCE_SHA256.into(),
            attribution: "FGVTN General Catalog, Click 3D Slow, printed p. 23".into(),
            plate_height: Length::ZERO,
            overlay_by_cup_edge: (3..=6)
                .map(|k| OverlaySetting {
                    cup_edge_setback: mm(k),
                    overlay: mm(k + 12),
                })
                .collect(),
            door_thickness_min: mm(15),
            door_thickness_max: mm(22),
            cup_diameter: mm(35),
            cup_depth: Length::from_micrometres(11_300),
            plate_hole_pitch: mm(32),
            plate_front_offset: mm(37),
            opening_limit_degrees: 105,
            screw_details: UnavailableDetail::Unavailable,
        }),
    }
}

/// A new ID belongs to the project snapshot, never to the bundled catalog.
pub fn builtin_hinge() -> CatalogReference {
    reviewed_2025_hinge()
}

/// A typed supported claim must match the reviewed source and *all* dimensions.
/// User-provided legacy records remain usable as references, never as a verified kit.
pub fn is_verified(entry: &CatalogReference) -> bool {
    let baseline = reviewed_2025_hinge();
    entry.product_id == baseline.product_id
        && entry.plate_id == baseline.plate_id
        && entry.source == baseline.source
        && entry.revision == baseline.revision
        && entry.installation_dimensions.is_empty()
        && entry.verified_hinge == baseline.verified_hinge
}

pub fn supported_setting(entry: &CatalogReference, setback: Length, overlay: Length) -> bool {
    is_verified(entry)
        && entry.verified_hinge.as_ref().is_some_and(|facts| {
            facts
                .overlay_by_cup_edge
                .iter()
                .any(|setting| setting.cup_edge_setback == setback && setting.overlay == overlay)
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogEditError {
    MissingEntry,
    InvalidBuiltin,
    DependentInstallation(String),
}

/// Add the bundled record as an independent project-owned snapshot.
pub fn add_builtin(editor: &mut ProjectEditor) -> Result<Uuid, EditError<CatalogEditError>> {
    let entry = builtin_hinge();
    let id = entry.id;
    editor.transact(|project| {
        project.catalog.push(entry);
        Ok(())
    })?;
    Ok(id)
}

/// Explicit replacement, retaining the project-local ID and recomputing every
/// dependent installation's diagnostics in the same candidate transaction.
pub fn update_from_builtin_with_status(
    editor: &mut ProjectEditor,
    catalog_id: Uuid,
) -> Result<(bool, Vec<InstallationStatus>), EditError<CatalogEditError>> {
    let mut statuses = Vec::new();
    let changed = update_from_builtin(editor, catalog_id, |candidate, _| {
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

/// Compatibility hook for existing hardware callers. Installation diagnosis
/// always runs before the hook, regardless of what the caller does with it.
pub fn update_from_builtin(
    editor: &mut ProjectEditor,
    catalog_id: Uuid,
    mut validate_installations: impl FnMut(&Project, &[Uuid]) -> Result<(), String>,
) -> Result<bool, EditError<CatalogEditError>> {
    let mut replacement = builtin_hinge();
    if !is_verified(&replacement) {
        return Err(EditError::Command(CatalogEditError::InvalidBuiltin));
    }
    replacement.id = catalog_id;
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
    use crate::domain::{DomainError, Hardware, HardwareKind};
    use crate::money::Currency;
    use crate::persistence::{prepare_bytes, save};
    use crate::units::{Pose, Quaternion};

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
    fn unsupported_variants_and_modified_facts_cannot_borrow_baseline() {
        let baseline = builtin_hinge();
        for change in 0..5 {
            let mut entry = baseline.clone();
            match change {
                0 => entry.product_id = "51MX153DCV00100".into(),
                1 => entry.plate_id = None,
                2 => entry.verified_hinge.as_mut().unwrap().cup_depth = mm(12),
                3 => entry.verified_hinge.as_mut().unwrap().source_sha256 = "other".into(),
                _ => {
                    entry
                        .installation_dimensions
                        .insert("pilot_depth".into(), mm(8));
                }
            }
            let mut project = Project::new("Unsupported", Currency::Brl);
            project.catalog.push(entry.clone());
            assert!(!is_verified(&entry));
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
