use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui;
use plan_my_cabinet::export::ExportStatus;
use plan_my_cabinet::i18n::{Language, Localizer};
use plan_my_cabinet::recent_projects::{
    RecentEntry, RecentProjectView, RecentStatus, RecentSummary, THUMBNAIL_SIZE,
};
use plan_my_cabinet::recovery::{DiscoveryStatus, RecoveryDiscovery, RecoveryIdentity};
use plan_my_cabinet::welcome_ui::{
    RecentAction, RecoveryAction, RecoveryBadge, WelcomeIntent, WelcomeState, recent_intent,
    recovery_badge, recovery_intent,
};
use uuid::Uuid;

fn row(path: &str, id: Uuid, status: RecentStatus) -> RecentProjectView {
    RecentProjectView {
        entry: RecentEntry {
            path: PathBuf::from(path),
            project_id: id,
            last_used_unix_ms: 1_779_000_000_000,
            cached_summary: RecentSummary {
                name: "Same name".into(),
                boards: 999,
                assemblies: 999,
                materials: 999,
                stock: 999,
                export_records: 999,
            },
            thumbnail_key: None,
        },
        status,
        export_status: None,
        thumbnail: None,
    }
}

fn available() -> RecentStatus {
    RecentStatus::Available(RecentSummary {
        name: "Same name".into(),
        boards: 2,
        assemblies: 1,
        materials: 1,
        stock: 0,
        export_records: 0,
    })
}

#[test]
fn equal_names_distinguish_path_and_uuid_and_unavailable_cannot_open() {
    let id = Uuid::new_v4();
    let first = row("/projects/a.pmcab", id, available());
    let copy = row("/projects/b.pmcab", id, available());
    let replaced = row("/projects/a.pmcab", Uuid::new_v4(), available());
    let intents =
        [&first, &copy, &replaced].map(|row| recent_intent(row, RecentAction::Open).unwrap());
    assert_ne!(intents[0], intents[1]);
    assert_ne!(intents[0], intents[2]);
    assert!(
        recent_intent(
            &row("/missing", id, RecentStatus::Missing),
            RecentAction::Open
        )
        .is_none()
    );
    assert!(
        recent_intent(
            &row("/invalid", id, RecentStatus::Unavailable("bad data".into())),
            RecentAction::Open
        )
        .is_none()
    );
}

#[test]
fn locate_remove_are_only_identity_intents_and_do_not_touch_files() {
    let dir = std::env::temp_dir().join(format!("pmcab-welcome-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let project = dir.join("cabinet.pmcab");
    let recovery = dir.join("recovery.json");
    fs::write(&project, b"project bytes").unwrap();
    fs::write(&recovery, b"recovery bytes").unwrap();
    let id = Uuid::new_v4();
    let missing = row(project.to_str().unwrap(), id, RecentStatus::Missing);
    assert_eq!(
        recent_intent(&missing, RecentAction::Locate),
        Some(WelcomeIntent::Locate {
            path: project.clone(),
            project_id: id
        })
    );
    assert_eq!(
        recent_intent(&missing, RecentAction::Remove),
        Some(WelcomeIntent::Remove {
            path: project.clone(),
            project_id: id
        })
    );
    assert!(
        recent_intent(
            &row(project.to_str().unwrap(), id, available()),
            RecentAction::Locate
        )
        .is_none()
    );
    assert_eq!(fs::read(&project).unwrap(), b"project bytes");
    assert_eq!(fs::read(&recovery).unwrap(), b"recovery bytes");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn filter_uses_validated_name_or_historical_name_and_path() {
    let id = Uuid::new_v4();
    let rows = [
        row(
            "/projects/current.pmcab",
            id,
            RecentStatus::Available(RecentSummary {
                name: "Atual".into(),
                boards: 0,
                assemblies: 0,
                materials: 0,
                stock: 0,
                export_records: 0,
            }),
        ),
        row("/projects/missing.pmcab", id, RecentStatus::Missing),
    ];
    let mut state = WelcomeState::default();
    state.filter = "same name".into();
    assert_eq!(state.filtered(&rows).len(), 1);
    state.filter = "ATUAL".into();
    assert_eq!(state.filtered(&rows).len(), 1);
    state.filter = "MISSING.PMCAB".into();
    assert_eq!(state.filtered(&rows).len(), 1);
}

#[test]
fn recovery_requires_both_identity_parts_and_discovery() {
    let id = Uuid::new_v4();
    let row = row("/projects/copy.pmcab", id, available());
    let discovery = RecoveryDiscovery {
        identity: RecoveryIdentity {
            project_id: id,
            saved_path: Some(PathBuf::from("/projects/original.pmcab")),
        },
        snapshot_path: PathBuf::from("/recovery/snapshot.json"),
        snapshot_modified: None,
        saved_revision: Some(1),
        recovery_revision: Some(2),
        status: DiscoveryStatus::Newer,
    };
    assert_eq!(recovery_badge(&row, None), RecoveryBadge::Unknown);
    assert_eq!(
        recovery_badge(&row, Some(&[discovery])),
        RecoveryBadge::NoneKnown
    );
    let discovery = RecoveryDiscovery {
        identity: RecoveryIdentity {
            project_id: id,
            saved_path: Some(row.entry.path.clone()),
        },
        snapshot_path: PathBuf::from("/recovery/snapshot.json"),
        snapshot_modified: None,
        saved_revision: Some(1),
        recovery_revision: Some(2),
        status: DiscoveryStatus::Newer,
    };
    assert_eq!(
        recovery_badge(&row, Some(&[discovery])),
        RecoveryBadge::Available
    );
}

fn discovery(status: DiscoveryStatus, saved_path: Option<&str>) -> RecoveryDiscovery {
    RecoveryDiscovery {
        identity: RecoveryIdentity {
            project_id: Uuid::new_v4(),
            saved_path: saved_path.map(PathBuf::from),
        },
        snapshot_path: PathBuf::from("/recovery/snapshot.json"),
        snapshot_modified: Some(SystemTime::now()),
        saved_revision: saved_path.map(|_| 4),
        recovery_revision: Some(5),
        status,
    }
}

#[test]
fn recovery_decisions_carry_exact_identity_and_only_valid_candidates_are_actionable() {
    let original = discovery(DiscoveryStatus::Newer, Some("/projects/original.pmcab"));
    let copy = discovery(DiscoveryStatus::Newer, Some("/projects/copy.pmcab"));
    let untitled = discovery(DiscoveryStatus::Untitled, None);
    for candidate in [&original, &copy, &untitled] {
        for action in [
            RecoveryAction::Recover,
            RecoveryAction::DecideLater,
            RecoveryAction::Discard,
        ] {
            assert_eq!(
                recovery_intent(candidate, action),
                Some(WelcomeIntent::Recovery {
                    identity: candidate.identity.clone(),
                    snapshot_path: candidate.snapshot_path.clone(),
                    action,
                })
            );
        }
    }
    assert_ne!(
        recovery_intent(&original, RecoveryAction::Recover),
        recovery_intent(&copy, RecoveryAction::Recover)
    );
    assert_eq!(untitled.saved_revision, None);
    assert_eq!(untitled.identity.saved_path, None);
    for status in [
        DiscoveryStatus::NotNewer,
        DiscoveryStatus::SavedUnavailable("missing".into()),
        DiscoveryStatus::Invalid("corrupt".into()),
    ] {
        assert!(
            recovery_intent(
                &discovery(status, Some("/projects/original.pmcab")),
                RecoveryAction::Recover
            )
            .is_none()
        );
    }
}

#[test]
fn recovery_render_has_no_file_side_effects_and_handles_unknown_metadata_in_both_languages() {
    let dir = std::env::temp_dir().join(format!("pmcab-welcome-recovery-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let saved = dir.join("saved.pmcab");
    let snapshot = dir.join("snapshot.json");
    fs::write(&saved, b"saved bytes").unwrap();
    fs::write(&snapshot, b"snapshot bytes").unwrap();
    let mut newer = discovery(DiscoveryStatus::Newer, saved.to_str());
    newer.snapshot_path = snapshot.clone();
    newer.snapshot_modified = None;
    newer.saved_revision = None;
    let mut untitled = discovery(DiscoveryStatus::Untitled, None);
    untitled.snapshot_path = snapshot.clone();
    let candidates = [
        newer,
        untitled,
        discovery(DiscoveryStatus::Invalid("invalid".into()), Some("/missing")),
    ];
    for language in [Language::En, Language::PtBr] {
        let ctx = egui::Context::default();
        let mut state = WelcomeState::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 700.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.set_min_size(egui::vec2(1100.0, 700.0));
                assert!(
                    state
                        .show(ui, &Localizer::new(language), &[], Some(&candidates))
                        .is_empty()
                );
            },
        );
        assert!(!output.shapes.is_empty());
        output.textures_delta.clear();
    }
    assert_eq!(fs::read(&saved).unwrap(), b"saved bytes");
    assert_eq!(fs::read(&snapshot).unwrap(), b"snapshot bytes");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn welcome_recovery_translation_keys_have_bilingual_parity() {
    let en = include_str!("../i18n/en.ftl");
    let pt = include_str!("../i18n/pt-BR.ftl");
    let keys = |text: &str| -> std::collections::BTreeSet<String> {
        text.lines()
            .filter_map(|line| line.split_once(" = ").map(|(key, _)| key))
            .filter(|key| key.starts_with("welcome-recovery-"))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(keys(en), keys(pt));
    for key in keys(en) {
        for language in [Language::En, Language::PtBr] {
            assert!(!Localizer::new(language).text(&key).is_empty(), "{key:?}");
        }
    }
}

fn draw(width: f32, height: f32, rows: &[RecentProjectView], language: Language) {
    let ctx = egui::Context::default();
    let mut state = WelcomeState::default();
    for scrolled in [false, true] {
        let events = if scrolled {
            vec![
                egui::Event::PointerMoved(egui::pos2(width - 80.0, height - 100.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -250.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        } else {
            Vec::new()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, height),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_min_size(egui::vec2(width, height));
                assert!(
                    state
                        .show(ui, &Localizer::new(language), rows, None)
                        .is_empty()
                );
            },
        );
        assert!(!output.shapes.is_empty());
        assert!(output.shapes.len() < 20_000);
        let _ = ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        output.textures_delta.clear();
    }
}

#[test]
fn empty_populated_long_portuguese_and_narrow_frames_render() {
    draw(1100.0, 700.0, &[], Language::En);
    let id = Uuid::new_v4();
    let mut rows: Vec<_> = (0..40)
        .map(|n| {
            row(
                &format!("/projects/{n}-{}", "nome-muito-longo".repeat(5)),
                id,
                available(),
            )
        })
        .collect();
    rows[0].export_status = Some(ExportStatus::Unknown);
    rows[0].entry.thumbnail_key = Some("v1-test".into());
    rows[0].thumbnail = Some(vec![255; THUMBNAIL_SIZE[0] * THUMBNAIL_SIZE[1] * 4]);
    rows[1].status = RecentStatus::Missing;
    rows[2].status = RecentStatus::Unavailable("Documento inválido".into());
    for (width, height) in [(1100.0, 700.0), (620.0, 500.0)] {
        draw(width, height, &rows, Language::PtBr);
    }
}
