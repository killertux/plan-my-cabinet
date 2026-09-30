//! The agent API end to end, driven with the same JSON an MCP client sends:
//! a base cabinet from the template, two doors, stock, a cut plan, hinges,
//! door relationships, pictures, and a save/reopen round trip.
use plan_my_cabinet::service::dto::Change;
use plan_my_cabinet::service::{ErrorCode, Workspace, WorkspaceConfig};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn input<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("tool input deserializes")
}

fn workspace(dir: &std::path::Path) -> Workspace {
    Workspace::new(WorkspaceConfig {
        catalog_dir: Some(dir.join("catalogs")),
        user_data_dir: Some(dir.to_path_buf()),
        ..Default::default()
    })
}

fn result(change: Change<Value>) -> Value {
    assert!(change.committed);
    change.result
}

fn dump(name: &str, png: &[u8]) {
    if let Some(dir) = std::env::var_os("PMCAB_RENDER_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join(name), png).unwrap();
    }
}

#[test]
fn base_cabinet_with_two_doors_from_template_to_saved_file() {
    let dir = std::env::temp_dir().join(format!("pmcab-service-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = workspace(&dir);

    // Nothing is open yet.
    assert_eq!(ws.get_project().unwrap_err().code, ErrorCode::NoOpenProject);

    let generated = ws
        .generate_template(input(json!({
            "kind": "base",
            "name": "Kitchen base 800",
            "dimensions": { "width": 800, "depth": 580, "height": 720 }
        })))
        .unwrap();
    let boards = generated["boards"].as_array().unwrap();
    assert!(boards.len() >= 5, "{generated:#}");
    assert!(
        boards.iter().all(|b| !b["sheet"].is_null()),
        "every template board is on a sheet: {generated:#}"
    );

    // Two full-overlay doors at the front (the front is Y = 0, doors go to -Y).
    let doors = result(
        ws.create_boards(input(json!({
            "boards": [
                { "name": "Left door", "material": "White MDF 18", "length": 397, "width": 716,
                  "pose": { "x": 0, "y": 0, "z": 2, "rx": 90 }, "parent": "Kitchen base 800" },
                { "name": "Right door", "material": "White MDF 18", "length": 397, "width": 716,
                  "pose": { "x": 403, "y": 0, "z": 2, "rx": 90 }, "parent": "Kitchen base 800" }
            ]
        })))
        .unwrap(),
    );
    assert_eq!(doors["boards"].as_array().unwrap().len(), 2);

    // Seeing the result.
    let described = ws.describe_scene(input(json!({}))).unwrap();
    let text = described["text"].as_str().unwrap();
    assert!(text.contains("Left door"), "{text}");
    assert_eq!(described["overlaps"], json!([]), "{text}");
    let pictures = ws
        .render_views(input(json!({ "views": [
            { "view": "iso" },
            { "view": "front", "projection": "orthographic" },
            { "view": "iso", "hidden": ["Left door", "Right door"] }
        ]})))
        .unwrap();
    assert_eq!(pictures.images.len(), 3);
    for (i, png) in pictures.images.iter().enumerate() {
        assert_eq!(&png[1..4], b"PNG");
        dump(&format!("service_view_{i}.png"), png);
    }

    // Stock for the doors, then a verified cut plan.
    let added = result(ws.add_needed_sheets(input(json!({}))).unwrap());
    assert!(!added["added"].as_array().unwrap().is_empty(), "{added:#}");
    let diagnostics = ws.get_diagnostics().unwrap();
    assert_eq!(
        diagnostics["boards_with_problems"],
        json!([]),
        "{diagnostics:#}"
    );
    result(
        ws.set_stock_prices(input(
            json!({ "prices": [{ "stock": "all", "price": "320.00" }] }),
        ))
        .unwrap(),
    );
    let settings = ws
        .set_project_settings(input(
            json!({ "cut_fee": "4.50", "confirm_shop_kerf": true }),
        ))
        .unwrap();
    assert!(settings.result.shop_kerf_confirmed);
    let plan = ws
        .get_cut_plan(input(json!({ "include_cuts": true })))
        .unwrap();
    assert!(plan["estimate"]["total"].is_object(), "{plan:#}");
    let sheets = ws.render_sheets(input(json!({}))).unwrap();
    assert!(!sheets.images.is_empty());
    dump("service_sheet.png", &sheets.images[0]);

    let search = ws
        .optimize_cut_plan(input(
            json!({ "objective": "fewest_cuts", "max_seconds": 5 }),
        ))
        .unwrap();
    if !search["candidates"].as_array().unwrap().is_empty() {
        let applied = ws
            .apply_optimization(input(
                json!({ "search_id": search["search_id"], "candidate": 0 }),
            ))
            .unwrap();
        assert!(applied.committed);
    }

    // Hinges and doors.
    let suggestion = ws
        .suggest_hinges(input(json!({ "door": "Left door" })))
        .unwrap();
    assert_eq!(suggestion["mount"]["name"], "Left side", "{suggestion:#}");
    let hinges = result(
        ws.add_hinges(input(json!({ "door": "Left door" })))
            .unwrap(),
    );
    assert_eq!(hinges["hinges"].as_array().unwrap().len(), 2, "{hinges:#}");
    assert!(
        hinges["hinges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["ok"] == true),
        "{hinges:#}"
    );
    result(
        ws.add_hinges(input(json!({ "door": "Right door" })))
            .unwrap(),
    );
    let door = result(
        ws.create_door(input(json!({ "moving": "Left door" })))
            .unwrap(),
    );
    assert!(door["opening_limit_degrees"].is_number(), "{door:#}");
    result(
        ws.create_door(input(json!({ "moving": "Right door" })))
            .unwrap(),
    );
    let open = ws
        .render_door_opening(input(json!({ "door": "Left door", "angle_degrees": 80 })))
        .unwrap();
    dump("service_door_open.png", &open.images[0]);

    // Save, then reopen the file.
    let path = dir.join("kitchen-base.pmcab");
    let saved = ws.save_project(input(json!({ "path": path }))).unwrap();
    assert_eq!(saved["dirty"], false);
    let before = ws.get_project().unwrap();
    let mut again = workspace(&dir);
    let reopened = again.open_project(input(json!({ "path": path }))).unwrap();
    assert_eq!(reopened.counts, before.counts);
    assert_eq!(reopened.counts["doors"], 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn guards_and_errors_are_explicit() {
    let dir = std::env::temp_dir().join(format!("pmcab-service-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = workspace(&dir);
    ws.new_project(input(json!({ "name": "Shelf" }))).unwrap();
    let material = result(
        ws.create_material(input(
            json!({ "name": "Birch ply", "thickness": "18 mm", "grain": "length" }),
        ))
        .unwrap(),
    );
    assert!(material["material_id"].is_string());

    // Inch fractions that are not whole micrometres need consent.
    let rounding = ws
        .create_board(input(json!({ "name": "Shelf", "material": "Birch ply", "length": "23 5/8 in", "width": "11.00001" })))
        .unwrap_err();
    assert_eq!(rounding.code, ErrorCode::RoundingRequired);
    let board = ws
        .create_board(input(json!({ "name": "Shelf", "material": "Birch ply", "length": "23 5/8 in", "width": 300 })))
        .unwrap();
    assert!(!board.warnings.is_empty(), "no stock yet: {board:?}");

    // Unsaved work is protected.
    let blocked = ws
        .new_project(input(json!({ "name": "Other" })))
        .unwrap_err();
    assert_eq!(blocked.code, ErrorCode::UnsavedChanges);

    // A used material cannot be deleted.
    let in_use = ws
        .delete_material(input(json!({ "ref": "Birch ply" })))
        .unwrap_err();
    assert_eq!(in_use.code, ErrorCode::MaterialInUse);

    // Ambiguous names are refused with candidates.
    ws.duplicate_board(input(json!({ "board": "Shelf" })))
        .unwrap();
    let ambiguous = ws.get_board(input(json!({ "ref": "Shelf" }))).unwrap_err();
    assert_eq!(ambiguous.code, ErrorCode::AmbiguousRef);

    // Overlaps are reported.
    let described = ws.describe_scene(input(json!({}))).unwrap();
    assert_eq!(
        described["overlaps"].as_array().unwrap().len(),
        1,
        "{described:#}"
    );

    // Undo walks back one step per call.
    let undo = ws.undo(input(json!({}))).unwrap();
    assert_eq!(undo["steps_done"], 1);

    // Templates can be added to the open project.
    let added = ws
        .generate_template(input(json!({ "kind": "wall", "name": "Wall unit", "into": "current_project", "offset_mm": [1000, 0, 1400] })))
        .unwrap();
    assert!(added["boards"].as_array().unwrap().len() >= 5, "{added:#}");
    std::fs::remove_dir_all(&dir).ok();
}
