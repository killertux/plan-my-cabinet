use plan_my_cabinet::candidate_generation::SearchBudget;
use plan_my_cabinet::candidate_ranking::Objective;
use plan_my_cabinet::color_commands::ColorEditError;
use plan_my_cabinet::commands::{EditError, ProjectEditor};
use plan_my_cabinet::domain::{
    Allocation, Board, BoardGrain, Material, NEUTRAL_MATERIAL_COLOR, Project, SrgbColor, Stock,
    StockGrain, StockSource,
};
use plan_my_cabinet::export::{
    ExportMode, ExportSettings, ExportStatus, fingerprint, prepare_export_with_budget,
};
use plan_my_cabinet::i18n::Language;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::optimization_worker::{OptimizationWorker, WorkerMessage};
use plan_my_cabinet::persistence::{prepare_bytes, serialize};
use plan_my_cabinet::units::{Length, Pose, Quaternion, Unit};
use uuid::Uuid;

const LEGACY: &[u8] = include_bytes!("fixtures/schema-v1-cabinet.pmcab");

fn ready_editor() -> ProjectEditor {
    let mut p = Project::new("Color-only", Currency::Brl);
    let mm = |n: i64| Length::from_micrometres(n * 1000);
    let material_id = Uuid::new_v4();
    let board_id = Uuid::new_v4();
    let stock_id = Uuid::new_v4();
    p.materials.push(Material {
        default_band: None,
        kind: Default::default(),
        id: material_id,
        name: "Plywood".into(),
        default_thickness: mm(18),
        default_grain: BoardGrain::Unrestricted,
    });
    p.boards.push(Board {
        banding: Default::default(),
        id: board_id,
        name: "Side".into(),
        material_id,
        length: mm(100),
        width: mm(50),
        thickness: mm(18),
        grain_override: None,
        parent_id: None,
        pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
    });
    p.stock.push(Stock {
        id: stock_id,
        name: "Exact blank".into(),
        material_id,
        length: mm(100),
        width: mm(50),
        thickness: mm(18),
        grain: StockGrain::Nondirectional,
        source: StockSource::Owned,
        price: None,
        priority: 0,
        trim: [Length::ZERO; 4],
    });
    p.allocations.push(Allocation {
        id: Uuid::new_v4(),
        board_id,
        stock_id,
        origin: [Length::ZERO; 2],
        quarter_turn: false,
        locked: true,
    });
    p.confirmed_shop_kerf = Some(p.cutting_kerf);
    ProjectEditor::new(p).unwrap()
}

fn readiness(p: &Project) -> (Vec<plan_my_cabinet::export::ExportIssue>, usize) {
    let plan = prepare_export_with_budget(
        p,
        ExportSettings {
            language: Language::En,
            units: Unit::Mm,
        },
        ExportMode::ShopReady,
        100,
    )
    .unwrap();
    (plan.wood_issues, plan.witnesses.len())
}

#[test]
fn color_is_undoable_portable_and_manufacturing_neutral() {
    let mut editor = ready_editor();
    let original = editor.project().clone();
    let material_id = original.materials[0].id;
    let baseline = fingerprint(&original);
    let readiness_before = readiness(&original);
    assert_eq!(readiness_before.1, 1);
    assert_eq!(original.material_color(material_id), NEUTRAL_MATERIAL_COLOR);
    assert!(!editor.is_dirty());
    assert!(!editor.set_material_color(material_id, None).unwrap());
    assert!(!editor.can_undo());

    let color = SrgbColor([12, 135, 244]);
    assert!(editor.set_material_color(material_id, Some(color)).unwrap());
    assert!(editor.is_dirty());
    assert_eq!(editor.project().revision, original.revision + 1);
    assert_eq!(editor.project().material_color(material_id), color);
    assert_eq!(editor.project().materials, original.materials);
    assert_eq!(editor.project().boards, original.boards);
    assert_eq!(editor.project().stock, original.stock);
    assert_eq!(editor.project().allocations, original.allocations);
    assert_eq!(
        editor.project().confirmed_shop_kerf,
        original.confirmed_shop_kerf
    );
    assert_eq!(fingerprint(editor.project()), baseline);
    assert_eq!(readiness(editor.project()), readiness_before);
    assert!(!editor.set_material_color(material_id, Some(color)).unwrap());

    let bytes = serialize(editor.project()).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["material_colors"][material_id.to_string()],
        serde_json::json!([12, 135, 244])
    );
    let reopened = prepare_bytes(&bytes).unwrap().into_editor();
    assert_eq!(reopened.project().material_color(material_id), color);
    assert!(!reopened.is_dirty());
    assert_eq!(fingerprint(reopened.project()), baseline);

    assert!(editor.undo().unwrap());
    assert_eq!(
        editor.project().material_color(material_id),
        NEUTRAL_MATERIAL_COLOR
    );
    assert!(!editor.is_dirty());
    assert_eq!(fingerprint(editor.project()), baseline);
    assert!(editor.redo().unwrap());
    assert_eq!(editor.project().material_color(material_id), color);
    assert!(editor.set_material_color(material_id, None).unwrap());
    assert_eq!(
        editor.project().material_color(material_id),
        NEUTRAL_MATERIAL_COLOR
    );
    assert_eq!(fingerprint(editor.project()), baseline);
    assert_eq!(readiness(editor.project()), readiness_before);
    editor.undo().unwrap();
    assert_eq!(editor.project().material_color(material_id), color);
}

#[test]
fn legacy_receipt_stays_current_and_missing_colors_do_not_dirty_on_open() {
    let mut editor = prepare_bytes(LEGACY).unwrap().into_editor();
    let material_id = editor.project().materials[0].id;
    assert_eq!(
        editor.project().material_color(material_id),
        NEUTRAL_MATERIAL_COLOR
    );
    assert!(editor.project().material_colors.is_empty());
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    let baseline = fingerprint(editor.project());
    let status = ExportStatus::for_project(editor.project());
    assert_eq!(status, ExportStatus::Current);
    editor
        .set_material_color(material_id, Some(SrgbColor([0, 255, 128])))
        .unwrap();
    assert!(editor.is_dirty());
    assert_eq!(fingerprint(editor.project()), baseline);
    assert_eq!(ExportStatus::for_project(editor.project()), status);
    assert_eq!(editor.project().export_records.len(), 1);
    assert!(editor.project().allocations[0].locked);
    let saved = serialize(editor.project()).unwrap();
    assert_eq!(
        prepare_bytes(&saved)
            .unwrap()
            .project()
            .material_color(material_id),
        SrgbColor([0, 255, 128])
    );
    editor.undo().unwrap();
    assert!(!editor.is_dirty());
    assert_eq!(ExportStatus::for_project(editor.project()), status);
}

#[test]
fn invalid_color_records_and_unknown_material_commands_are_rejected_atomically() {
    let mut editor = ready_editor();
    let missing = Uuid::new_v4();
    assert_eq!(
        editor.set_material_color(missing, Some(SrgbColor([1, 2, 3]))),
        Err(EditError::Command(ColorEditError::MissingMaterial(missing)))
    );
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    let bytes = serialize(editor.project()).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for bad in [
        serde_json::json!([1, 2]),
        serde_json::json!([1, 2, 256]),
        serde_json::json!([1.5, 2, 3]),
    ] {
        document["material_colors"] =
            serde_json::json!({editor.project().materials[0].id.to_string(): bad});
        assert!(prepare_bytes(&serde_json::to_vec(&document).unwrap()).is_err());
    }
    document["material_colors"] = serde_json::json!({missing.to_string(): [1, 2, 3]});
    assert!(prepare_bytes(&serde_json::to_vec(&document).unwrap()).is_err());
    assert!(!editor.is_dirty());
}

#[test]
fn color_edit_keeps_in_flight_optimization_applicable() {
    let mut editor = ready_editor();
    let material_id = editor.project().materials[0].id;
    let allocations = editor.project().allocations.clone();
    let mut worker = OptimizationWorker::start(
        &editor,
        Objective::FewestCuts,
        SearchBudget {
            placements: 100,
            witness_states: 100,
            beam_width: 4,
        },
        std::time::Duration::from_secs(5),
    );
    let color = SrgbColor([210, 40, 80]);
    editor.set_material_color(material_id, Some(color)).unwrap();
    let result = (0..5000)
        .find_map(|_| {
            if let Some(WorkerMessage::Completed(result)) = worker.try_receive().unwrap() {
                Some(*result.unwrap())
            } else {
                std::thread::sleep(std::time::Duration::from_millis(1));
                None
            }
        })
        .expect("small optimization completed");
    assert!(result.is_current(editor.project()));
    let incumbent = result
        .ranking
        .candidates
        .iter()
        .position(|c| c.candidate.allocations == allocations)
        .expect("current exact-sheet plan is a candidate");
    assert_eq!(
        OptimizationWorker::apply(&mut editor, &result, incumbent),
        Ok(false)
    );
    assert_eq!(editor.project().material_color(material_id), color);
    assert_eq!(editor.project().allocations, allocations);
}
