//! Task 11.1: one physical cabinet from public editing APIs through shop handoff.
use std::{
    fs,
    fs::File,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use plan_my_cabinet::{
    board_commands::{NewBoard, NewMaterial},
    board_dimensions::BoardDimension,
    candidate_generation::{SearchBudget, validate_complete},
    candidate_ranking::Objective,
    commands::ProjectEditor,
    domain::{
        BoardEdge, BoardFace, BoardGrain, HingeInstallation, HingeMountingSide, Project,
        StockGrain, StockSource,
    },
    door_joint,
    export::{ExportMode, ExportSettings, Overwrite, prepare_export, write_pdf},
    hardware_catalog, hinge_installation,
    i18n::Language,
    material_changes::{ConflictReason, allocation_conflicts},
    measurements::{Frame, Scope, measure},
    money::{Currency, Money},
    optimization_worker::{OptimizationWorker, WorkerMessage},
    persistence::{prepare_reader, save},
    sheet_edit::{SheetEditError, SheetEditSession},
    stock_commands::StockInput,
    units::{Anchor, Length, Pose, Quaternion, Unit},
};
use uuid::Uuid;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}
fn pose(x: f64, y: f64, z: f64) -> Pose {
    Pose::new([x, y, z], Quaternion::IDENTITY).unwrap()
}
fn upright(x: f64) -> Pose {
    let half = std::f64::consts::FRAC_PI_4;
    Pose::new(
        [x, 0., 0.],
        Quaternion::normalized(half.cos(), 0., -half.sin(), 0.).unwrap(),
    )
    .unwrap()
}

#[test]
fn rectangular_cabinet_release_walkthrough() {
    let dir = std::env::temp_dir().join(format!("pmcab-release-{}", Uuid::new_v4()));
    fs::create_dir(&dir).unwrap();
    let mut editor =
        ProjectEditor::new(Project::new("Rectangular cabinet", Currency::Brl)).unwrap();
    let material = editor
        .create_material(NewMaterial {
            name: "18 mm plywood".into(),
            thickness: mm(18),
            grain: BoardGrain::Length,
        })
        .unwrap();
    editor.set_cutting_kerf(mm(5)).unwrap();
    editor.confirm_shop_kerf().unwrap();
    editor
        .set_cut_fee(Some(Money::new(Currency::Brl, 500).unwrap()))
        .unwrap();
    let mut stocks = Vec::new();
    for (name, length, width, price) in [
        ("Side A", 2300, 600, 20_000),
        ("Side B", 2300, 600, 20_000),
        ("Shelves", 1600, 600, 15_000),
        ("Door", 784, 600, 10_000),
    ] {
        stocks.push(
            editor
                .create_stock(
                    StockInput {
                        name: name.into(),
                        material_id: material,
                        length: mm(length),
                        width: mm(width),
                        thickness: mm(18),
                        grain: StockGrain::AlongX,
                        source: StockSource::ToPurchase,
                        price: Some(Money::new(Currency::Brl, price).unwrap()),
                        trim: [Length::ZERO; 4],
                    },
                    1,
                )
                .unwrap()[0],
        );
    }
    let (left, right, shelf) = {
        let mut board = |name: &str, length, at| {
            editor
                .create_board_with_fit(NewBoard {
                    name: name.into(),
                    material_id: material,
                    length: mm(length),
                    width: mm(600),
                    pose: at,
                })
                .unwrap()
                .0
        };
        (
            board("Left side", 2300, upright(18.)),
            board("Right side", 2300, upright(820.)),
            board("Shelf", 784, pose(18., 0., 1100.)),
        )
    };
    let shelf_copy = editor.duplicate_board(shelf, pose(18., 0., 1500.)).unwrap();
    let door = editor
        .create_board_with_fit(NewBoard {
            name: "Door".into(),
            material_id: material,
            length: mm(784),
            width: mm(600),
            pose: pose(18., -18., 0.),
        })
        .unwrap()
        .0;
    assert_ne!(shelf, shelf_copy);
    assert_eq!(editor.project().allocations.len(), 5);
    let body = editor
        .group_objects(&[left, right, shelf, shelf_copy], None, "Body", [0.; 3])
        .unwrap();
    let door_group = editor
        .group_objects(&[door], None, "Door assembly", [0.; 3])
        .unwrap();
    let feet: Vec<_> = [(0., 0.), (802., 0.), (0., 560.), (802., 560.)]
        .into_iter()
        .map(|(x, y)| {
            editor
                .create_placeholder(
                    "Foot".into(),
                    [mm(18), mm(40), mm(100)],
                    None,
                    pose(x, y, -100.),
                )
                .unwrap()
        })
        .collect();
    let body_bounds = measure(editor.project(), &[body], Scope::Body, Frame::World).unwrap();
    let overall = measure(
        editor.project(),
        &[body, door_group, feet[0], feet[1], feet[2], feet[3]],
        Scope::Overall,
        Frame::World,
    )
    .unwrap();
    assert_eq!(body_bounds.dimensions_mm[2], 2300.);
    assert_eq!(overall.dimensions_mm[2], 2400.);
    assert_eq!(editor.project().boards.len(), 5); // feet are not cut parts

    let catalog = hardware_catalog::add_builtin(&mut editor).unwrap();
    let hinge_id = Uuid::new_v4();
    let status = hinge_installation::create(
        &mut editor,
        HingeInstallation {
            id: hinge_id,
            door_board_id: door,
            mounting_board_id: left,
            catalog_id: catalog,
            side: HingeMountingSide {
                door_edge: BoardEdge::MinX,
                door_face: BoardFace::MinZ,
                mount_front_edge: BoardEdge::MinX,
                mount_face: BoardFace::MaxZ,
            },
            door_y: mm(100),
            mount_y: mm(100),
            cup_edge_setback: mm(3),
            overlay: mm(15),
        },
    )
    .unwrap();
    assert!(status.issues.is_empty());
    assert_eq!(
        status.references.unwrap().product_id,
        hardware_catalog::KIT_ID
    );
    let joint = door_joint::preview(
        editor.project(),
        Uuid::new_v4(),
        door_group,
        left,
        vec![hinge_id],
    )
    .unwrap();
    door_joint::confirm(&mut editor, joint).unwrap();
    let closed = editor.project().clone();
    let moving =
        door_joint::derived_poses(editor.project(), &editor.project().door_joints[0], 90.).unwrap();
    assert!(moving.iter().any(|(id, _)| *id == door));
    assert_eq!(editor.project(), &closed); // approximate motion is display-only

    // A locked shelf keeps its stale sheet position after a design resize.
    {
        let mut session = SheetEditSession::begin(&mut editor);
        session.set_lock(shelf_copy, true).unwrap();
        session.accept().unwrap();
    }
    let old_copy = editor
        .project()
        .boards
        .iter()
        .find(|b| b.id == shelf_copy)
        .unwrap()
        .length;
    let resize = editor
        .preview_board_dimension(shelf, BoardDimension::Length, mm(800), Anchor::Start)
        .unwrap();
    let conflicts = editor.edit_board_dimension(resize).unwrap();
    assert!(
        conflicts
            .iter()
            .any(|c| c.board_id == shelf_copy && c.reasons.contains(&ConflictReason::Overlap))
    );
    assert_eq!(old_copy, mm(784));
    assert!(
        prepare_export(
            editor.project(),
            ExportSettings {
                language: Language::En,
                units: Unit::Mm
            },
            ExportMode::ShopReady
        )
        .is_err()
    );
    {
        let mut session = SheetEditSession::begin(&mut editor);
        assert_eq!(
            session.place(shelf_copy, stocks[2], [mm(805), mm(0)], false),
            Err(SheetEditError::Locked(shelf_copy))
        );
        session.set_lock(shelf_copy, false).unwrap();
        session
            .place(shelf_copy, stocks[2], [mm(805), mm(0)], false)
            .unwrap();
        session.set_lock(shelf_copy, true).unwrap();
        session.accept().unwrap();
    }
    assert!(allocation_conflicts(editor.project()).is_empty());
    let locked = editor
        .project()
        .allocations
        .iter()
        .find(|a| a.board_id == shelf_copy)
        .unwrap()
        .clone();

    let mut worker = OptimizationWorker::start(
        &editor,
        Objective::LowestNewSpending,
        SearchBudget {
            placements: 10_000,
            witness_states: 20_000,
            beam_width: 8,
        },
        Duration::from_secs(5),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let result = loop {
        assert!(
            Instant::now() < deadline,
            "optimization worker did not finish"
        );
        if let Some(WorkerMessage::Completed(result)) = worker.try_receive().unwrap() {
            break *result.unwrap();
        }
        thread::sleep(Duration::from_millis(2));
    };
    assert!(result.is_current(editor.project()));
    assert!(!result.ranking.candidates.is_empty());
    assert!(result.ranking.candidates[0].new_spending.is_some());
    for ranked in &result.ranking.candidates {
        validate_complete(editor.project(), &ranked.candidate).unwrap();
        assert_eq!(
            ranked
                .candidate
                .allocations
                .iter()
                .find(|a| a.board_id == shelf_copy)
                .unwrap(),
            &locked
        );
    }
    let chosen = result.ranking.candidates[0].candidate.allocations.clone();
    OptimizationWorker::apply(&mut editor, &result, 0).unwrap();
    assert_eq!(editor.project().allocations, chosen);

    for (language, filename) in [
        (Language::En, "cabinet-en.pdf"),
        (Language::PtBr, "cabinet-pt.pdf"),
    ] {
        let settings = ExportSettings {
            language,
            units: Unit::Mm,
        };
        let prepared = prepare_export(editor.project(), settings, ExportMode::ShopReady).unwrap();
        assert_eq!(prepared.witnesses.len(), 4);
        assert_eq!(prepared.installation_guidance.len(), 1);
        let path = dir.join(filename);
        let output = write_pdf(&prepared, Some(&path), Overwrite::Decline).unwrap();
        assert!(fs::read(&path).unwrap().starts_with(b"%PDF-"));
        if let Ok(text) = Command::new("pdftotext")
            .arg("-layout")
            .arg(&path)
            .arg("-")
            .output()
        {
            assert!(text.status.success());
            let text = String::from_utf8(text.stdout).unwrap();
            for label in match language {
                Language::En => ["NOT A CUTTING TEMPLATE", "Hinge installation", "Hardware"],
                Language::PtBr => [
                    "NÃO É GABARITO DE CORTE",
                    "Instalação da dobradiça",
                    "Ferragens",
                ],
            } {
                assert!(text.contains(label), "{filename}: missing {label}");
            }
            assert!(text.contains(hardware_catalog::KIT_ID));
            assert!(text.contains(&shelf_copy.to_string()));
        }
        editor
            .record_completed_export(
                &prepared.snapshot,
                &path,
                output.completed_unix_ms,
                &output.file_sha256,
            )
            .unwrap();
    }
    assert_ne!(
        fs::read(dir.join("cabinet-en.pdf")).unwrap(),
        fs::read(dir.join("cabinet-pt.pdf")).unwrap()
    );
    let project_path = dir.join("cabinet.pmcab");
    save(&mut editor, &project_path).unwrap();
    let reopened = prepare_reader(File::open(&project_path).unwrap())
        .unwrap()
        .into_editor();
    assert_eq!(reopened.project().boards, editor.project().boards);
    assert_eq!(reopened.project().allocations, editor.project().allocations);
    assert_eq!(reopened.project().hardware, editor.project().hardware);
    assert_eq!(reopened.project().catalog, editor.project().catalog);
    assert_eq!(
        reopened.project().hinge_installations,
        editor.project().hinge_installations
    );
    assert_eq!(reopened.project().door_joints, editor.project().door_joints);
    assert_eq!(
        reopened.project().export_records,
        editor.project().export_records
    );
    prepare_export(
        reopened.project(),
        ExportSettings {
            language: Language::PtBr,
            units: Unit::Mm,
        },
        ExportMode::ShopReady,
    )
    .unwrap();
    fs::remove_dir_all(dir).unwrap();
}
