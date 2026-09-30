use super::*;
use crate::domain::{HingeArm, OverlaySetting, UnavailableDetail};

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1_000)
}

const MINIMAL: &str = r#"
schema = 1
id = "acme"
manufacturer = "Acme"
version = "1"
review = { status = "reviewed" }

[[sources]]
id = "sheet"
title = "Acme sheet"
url = "https://example.com/sheet.pdf"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
revision = "2026"

[[hinges]]
id = "h"
name = { en = "Hinge" }
source = "sheet"
opening_degrees = 110
cup = { diameter = 35, depth = 11.5 }
door_thickness = { min = 16, max = 22 }
plate = { front_offset = 37, hole_pitch = 32 }

  [[hinges.variants]]
  arm = "full_overlay"
  code = "A-1"
  plate_height = 0
  k_table = [[3, 14], [4, 15]]

  [[hinges.variants]]
  arm = "inset"
  code = "A-2"
  plate_height = 1.5
  k_table = [[3, 2], [4, 1], [5, 0]]
"#;

fn user(text: &str) -> LoadedPack {
    load(PackOrigin::User("test.toml".into()), text.as_bytes())
}

fn codes(pack: &LoadedPack) -> Vec<(&str, &str)> {
    pack.issues
        .iter()
        .map(|i| (i.path.as_str(), i.kind.code()))
        .collect()
}

#[test]
fn bundled_packs_load_without_errors_or_warnings() {
    let registry = CatalogRegistry::bundled();
    assert!(!registry.packs.is_empty());
    for loaded in &registry.packs {
        assert_eq!(loaded.issues, [], "{}", loaded.file_name());
        let pack = loaded.usable().expect("usable bundled pack");
        let expected = if pack.feet.is_empty() {
            ReviewStatus::Reviewed
        } else {
            ReviewStatus::Generic
        };
        assert_eq!(pack.review.status, expected);
        for family in &pack.slides {
            for variant in &family.variants {
                let entry = snapshot_slide(pack, family, variant, "pt-BR");
                assert!(
                    crate::hardware_catalog::is_usable(&entry),
                    "{}",
                    variant.code
                );
                assert_eq!(
                    crate::hardware_catalog::trust(&entry),
                    Some(crate::hardware_catalog::Trust::Reviewed)
                );
            }
        }
        for family in &pack.feet {
            for variant in &family.variants {
                let entry = snapshot_foot(pack, family, variant, "en");
                assert!(
                    crate::hardware_catalog::is_usable(&entry),
                    "{}",
                    variant.code
                );
                assert_eq!(
                    crate::hardware_catalog::trust(&entry),
                    Some(crate::hardware_catalog::Trust::Generic)
                );
            }
        }
        for family in &pack.hinges {
            for variant in &family.variants {
                let entry = snapshot(pack, family, variant, "pt-BR");
                assert!(
                    crate::hardware_catalog::is_verified(&entry),
                    "{}",
                    variant.code
                );
                assert_eq!(
                    crate::hardware_catalog::trust(&entry),
                    Some(crate::hardware_catalog::Trust::Reviewed)
                );
            }
        }
    }
}

#[test]
fn fgvtn_pack_covers_every_reviewed_sheet() {
    let registry = CatalogRegistry::bundled();
    let pack = registry.pack("fgvtn").unwrap();
    assert_eq!(pack.hinges.len(), 8);
    let variants: Vec<_> = pack.hinges.iter().flat_map(|f| &f.variants).collect();
    // 7 sheets × 3 arms + the general-catalog kit, minus the doubtful Alta.
    assert_eq!(variants.len(), 21);
    let inset = variants.iter().filter(|v| v.arm == HingeArm::Inset).count();
    assert_eq!(inset, 6);
    let ms = pack
        .hinges
        .iter()
        .find(|f| f.id == "ms-slow-calco-fixo")
        .unwrap();
    assert_eq!(ms.cup_depth.micrometres(), 9_500);
    assert_eq!(
        ms.variants[1].k_table[0],
        (mm(3), Length::from_micrometres(5_500))
    );
    assert_eq!(ms.variants[0].plate_height, mm(4));
    let caneco = pack
        .hinges
        .iter()
        .find(|f| f.id == "tn-ms-slow-calco-fixo-caneco-9-8")
        .unwrap();
    assert_eq!(caneco.cup_depth.micrometres(), 9_800);
    assert_eq!(caneco.plate_hole_pitch, None);
    // Every source cites a SHA-256 and the doubtful rows stay out.
    let easy = pack
        .hinges
        .iter()
        .find(|f| f.id == "easy-tn-click-slow-inox-calco-duplo")
        .unwrap();
    assert_eq!(easy.variants[1].k_table[0].0, mm(4));
    assert!(!variants.iter().any(|v| v.code == "51MS15XTNSS15CF"));
}

#[test]
fn reviewed_kit_snapshot_equals_the_previous_rust_record() {
    let entry = crate::hardware_catalog::builtin_hinge();
    assert_eq!(
        entry.name,
        "FGVTN Click 3D Slow Reta / Calço 0 (complete kit)"
    );
    assert_eq!(entry.product_id, crate::hardware_catalog::KIT_ID);
    assert_eq!(
        entry.plate_id.as_deref(),
        Some(crate::hardware_catalog::PLATE_ID)
    );
    assert_eq!(entry.source, crate::hardware_catalog::SOURCE_URL);
    assert_eq!(
        entry.revision,
        format!(
            "May 2025 catalog; modified 2026-09-16; SHA-256 {}",
            crate::hardware_catalog::SOURCE_SHA256
        )
    );
    assert!(entry.installation_dimensions.is_empty());
    assert_eq!(
        entry.verified_hinge,
        Some(VerifiedHinge {
            printed_page: 23,
            pdf_page: 14,
            source_sha256: crate::hardware_catalog::SOURCE_SHA256.into(),
            attribution: "FGVTN General Catalog, Click 3D Slow, printed p. 23".into(),
            arm: HingeArm::FullOverlay,
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
        })
    );
    let origin = entry.origin.unwrap();
    assert_eq!(
        (origin.pack_id.as_str(), origin.item_id.as_str()),
        ("fgvtn", "click-3d-slow")
    );
}

#[test]
fn minimal_pack_converts_decimals_exactly() {
    let loaded = user(MINIMAL);
    assert_eq!(loaded.issues, []);
    let pack = loaded.usable().unwrap();
    let family = &pack.hinges[0];
    assert_eq!(family.cup_depth.micrometres(), 11_500);
    assert_eq!(family.variants[1].plate_height.micrometres(), 1_500);
    // A zero gap is valid for an inset arm.
    assert_eq!(family.variants[1].k_table[2], (mm(5), Length::ZERO));
    let entry = snapshot(pack, family, &family.variants[1], "en");
    assert_eq!(entry.name, "Acme Hinge");
    assert_eq!(entry.verified_hinge.as_ref().unwrap().arm, HingeArm::Inset);
    assert_eq!(
        crate::hardware_catalog::trust(&entry),
        Some(crate::hardware_catalog::Trust::UserSupplied)
    );
}

#[test]
fn every_problem_is_reported_with_its_path() {
    let text = MINIMAL
        .replace("depth = 11.5", "depth = 16")
        .replace("[[3, 14], [4, 15]]", "[[4, 14], [3, 15.0001]]")
        .replace("code = \"A-2\"", "code = \"A-1\"")
        .replace("min = 16, max = 22", "min = 16, max = 12")
        .replace("opening_degrees = 110", "opening_degrees = 200");
    let loaded = user(&text);
    assert!(loaded.usable().is_none());
    let found = codes(&loaded);
    for expected in [
        ("hinges[0].door_thickness", "thickness-range"),
        ("hinges[0].opening_degrees", "opening-angle"),
        ("hinges[0].variants[0].k_table[1]", "invalid-length"),
        ("hinges[0].variants[0].k_table", "k-not-increasing"),
        ("hinges[0].variants[1].code", "duplicate-code"),
    ] {
        assert!(found.contains(&expected), "{expected:?} not in {found:?}");
    }
    // Text for the command line names severity, path and detail.
    let line = loaded.issues[0].to_string();
    assert!(line.starts_with("error: hinges[0]"), "{line}");
}

#[test]
fn cup_deeper_than_the_thinnest_door_is_an_error() {
    let loaded = user(&MINIMAL.replace("depth = 11.5", "depth = 16"));
    assert!(codes(&loaded).contains(&("hinges[0].cup.depth", "cup-deeper-than-door")));
}

#[test]
fn syntax_schema_and_unknown_fields_are_errors_not_panics() {
    for (text, code) in [
        ("schema = ", "syntax"),
        (
            "schema = 2\nid='a'\nmanufacturer='a'\nversion='1'\nreview={status='draft'}",
            "unsupported-schema",
        ),
        (&MINIMAL.replace("mounting", "mount") as &str, "ok"),
        (
            &MINIMAL.replace("opening_degrees = 110", "opening = 110") as &str,
            "syntax",
        ),
        (
            &MINIMAL.replace("source = \"sheet\"", "source = \"nope\"") as &str,
            "unknown-source",
        ),
    ] {
        let loaded = user(text);
        if code == "ok" {
            continue;
        }
        assert!(
            loaded.issues.iter().any(|i| i.kind.code() == code),
            "{code}: {:?}",
            loaded.issues
        );
        assert!(loaded.usable().is_none());
    }
}

#[test]
fn warnings_keep_a_pack_usable_and_allow_needs_a_reason() {
    let uneven = MINIMAL.replace("[[3, 2], [4, 1], [5, 0]]", "[[3, 3], [4, 1], [5, 0]]");
    let loaded = user(&uneven);
    assert_eq!(
        codes(&loaded),
        [("hinges[0].variants[1].k_table", "not-monotonic")]
    );
    assert!(loaded.usable().is_some());
    let allowed = uneven.replace(
        "opening_degrees = 110",
        "opening_degrees = 110\nallow = [{ warning = \"not-monotonic\", reason = \"As printed\" }]",
    );
    assert_eq!(user(&allowed).issues, []);
    let no_reason = allowed.replace("As printed", " ");
    assert_eq!(user(&no_reason).issues.len(), 1);
    let draft = MINIMAL.replace("status = \"reviewed\"", "status = \"draft\"");
    assert_eq!(codes(&user(&draft)), [("review.status", "draft")]);
}

#[test]
fn user_folder_packs_load_sorted_and_shadow_bundled_ids() {
    let dir = std::env::temp_dir().join(format!("packs-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("b.toml"), MINIMAL).unwrap();
    std::fs::write(dir.join("a.toml"), "not toml [").unwrap();
    std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
    let shadow = MINIMAL.replace("id = \"acme\"", "id = \"fgvtn\"");
    std::fs::write(dir.join("c.TOML"), shadow).unwrap();
    let registry = CatalogRegistry::load(Some(&dir));
    std::fs::remove_dir_all(&dir).unwrap();
    let names: Vec<_> = registry.packs.iter().map(LoadedPack::file_name).collect();
    let bundled = BUNDLED.len();
    assert_eq!(
        names[bundled..],
        ["a.toml", "b.toml", "c.TOML"].map(String::from)
    );
    assert!(registry.packs[bundled].usable().is_none());
    assert!(codes(&registry.packs[bundled + 2]).contains(&("id", "shadows-bundled")));
    // The user's pack is the one in effect for its id.
    assert_eq!(registry.pack("fgvtn").unwrap().manufacturer, "Acme");
    let ids: Vec<_> = registry.usable().iter().map(|p| p.id.clone()).collect();
    assert_eq!(ids, ["fgvtn-slides", "generic-feet", "acme", "fgvtn"]);
    // A missing folder simply has no user packs.
    let empty = CatalogRegistry::load(Some(Path::new("/nonexistent/packs")));
    assert_eq!(empty.packs.len(), CatalogRegistry::bundled().packs.len());
}

#[test]
fn oversized_files_are_refused_before_parsing() {
    let big = vec![b'#'; MAX_PACK_BYTES + 1];
    let loaded = user(std::str::from_utf8(&big).unwrap());
    assert_eq!(codes(&loaded), [("", "too-large")]);
}

#[test]
fn every_bundled_sheet_and_code_is_in_its_review_record() {
    let review = include_str!("../../../docs/catalogs/fgvtn-review.md");
    let registry = CatalogRegistry::bundled();
    let pack = registry.pack("fgvtn").unwrap();
    for source in pack
        .sources
        .iter()
        .filter(|s| s.id != "catalogo-geral-2025")
    {
        assert!(
            review.contains(&format!("`{}`", source.id)),
            "{}",
            source.id
        );
        assert!(review.contains(&source.sha256[..8]), "{}", source.id);
    }
    for family in pack.hinges.iter().filter(|f| f.id != "click-3d-slow") {
        for variant in &family.variants {
            // Codes are listed by their shared prefix and distinct suffixes.
            let suffix = &variant.code[variant.code.len() - 4..];
            assert!(review.contains(suffix), "{}", variant.code);
        }
    }
}

const SLIDES: &str = r#"
schema = 1
id = "acme-slides"
manufacturer = "Acme"
version = "1"
review = { status = "reviewed" }

[[sources]]
id = "sheet"
title = "Acme slide sheet"
url = "https://example.com/slides.pdf"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
revision = "2026"

[[drawer_slides]]
id = "s45"
name = { en = "Slide 45" }
source = "sheet"
height = 45
clearance = { nominal = 12.7, plus = 0.5 }
front_setback = 2

  [[drawer_slides.variants]]
  code = "S45-400"
  length = 400
  travel = 390
  cabinet_holes = [35, 51, { along = 99, offset = 8, diameter = 4.5 }]
  drawer_holes = [32, 48]
"#;

const FEET: &str = r##"
schema = 1
id = "acme-feet"
manufacturer = "Acme"
version = "1"
review = { status = "generic" }

[[feet]]
id = "post"
name = { en = "Post" }
shape = { kind = "post", height = 100, tube = { width = 30, depth = 30 }, plate = { diameter = 60 }, plate_thickness = 2, glide = { diameter = 35, height = 10 } }
color = "#c0c4c8"
mounting_holes = [[-20, 0], [20, 0]]

  [[feet.variants]]
  code = "P100"

  [[feet.variants]]
  code = "P60"
  height = 60
  color = "#202020"
"##;

#[test]
fn slide_and_foot_tables_load_to_exact_facts() {
    let slides = user(SLIDES);
    assert_eq!(slides.issues, []);
    let pack = slides.usable().unwrap();
    let family = &pack.slides[0];
    assert_eq!(family.clearance.micrometres(), 12_700);
    assert_eq!(family.clearance_plus.micrometres(), 500);
    let variant = &family.variants[0];
    assert_eq!(variant.cabinet_holes[2].offset, mm(8));
    let entry = snapshot_slide(pack, family, variant, "en");
    assert_eq!(entry.product_id, "S45-400");
    assert_eq!(entry.name, "Slide 45 400 mm");
    assert!(entry.slide().unwrap().is_consistent());

    let feet = user(FEET);
    assert_eq!(feet.issues, []);
    let pack = feet.usable().unwrap();
    let family = &pack.feet[0];
    let short = family.spec(&family.variants[1]);
    assert_eq!(short.local_size(), [mm(60), mm(60), mm(60)]);
    assert_eq!(short.color, crate::domain::SrgbColor([0x20, 0x20, 0x20]));
}

#[test]
fn slide_and_foot_problems_have_paths() {
    let bad = SLIDES
        .replace(
            "clearance = { nominal = 12.7, plus = 0.5 }",
            "clearance = { nominal = 12.7, minus = 13 }",
        )
        .replace("travel = 390", "travel = 600")
        .replace("drawer_holes = [32, 48]", "drawer_holes = [48, 32, 450]")
        .replace("source = \"sheet\"\nheight", "height");
    let codes_found = codes(&user(&bad))
        .into_iter()
        .map(|(p, c)| format!("{p}:{c}"))
        .collect::<Vec<_>>();
    for expected in [
        "drawer_slides[0].source:missing",
        "drawer_slides[0].clearance:clearance-range",
        "drawer_slides[0].variants[0].travel:travel-too-long",
        "drawer_slides[0].variants[0].drawer_holes[2]:hole-outside-member",
        "drawer_slides[0].variants[0].drawer_holes:holes-not-increasing",
    ] {
        assert!(
            codes_found.contains(&expected.to_owned()),
            "{expected} in {codes_found:?}"
        );
    }

    let bad = FEET
        .replace("kind = \"post\"", "kind = \"cube\"")
        .replace("#c0c4c8", "grey");
    let loaded = user(&bad);
    let found = codes(&loaded);
    assert!(found.contains(&("feet[0].shape.kind", "unknown-shape")));
    assert!(found.contains(&("feet[0].color", "invalid-color")));

    let tall_glide = FEET.replace("height = 10 }", "height = 99 }");
    assert!(codes(&user(&tall_glide)).contains(&("feet[0].variants[0]", "shape-geometry")));
    let missing = FEET.replace("plate_thickness = 2, ", "");
    assert!(codes(&user(&missing)).contains(&("feet[0].shape.plate_thickness", "missing")));
    let unknown = FEET.replace("mounting_holes", "screw_holes");
    assert!(user(&unknown).pack.is_none());
}

#[test]
fn slide_review_and_guides_cite_the_bundled_slide_pack() {
    let review = include_str!("../../../docs/catalogs/fgvtn-slides-review.md");
    let guides = [
        include_str!("../../../docs/hardware-en.md"),
        include_str!("../../../docs/hardware-pt-BR.md"),
    ];
    let registry = CatalogRegistry::bundled();
    let pack = registry.pack("fgvtn-slides").unwrap();
    for source in &pack.sources {
        assert!(review.contains(&source.sha256), "{}", source.id);
        assert!(
            review.contains(&format!("`{}`", source.id)),
            "{}",
            source.id
        );
    }
    for guide in guides {
        assert!(guide.contains("0073.045500SX"));
        assert!(guide.contains("TT90"));
        assert!(guide.contains("12.7") || guide.contains("12,7"));
    }
    let feet = registry.pack("generic-feet").unwrap();
    assert_eq!(feet.review.status, ReviewStatus::Generic);
}
