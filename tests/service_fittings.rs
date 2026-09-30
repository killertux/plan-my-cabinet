//! Slides and feet through the agent API, with the JSON an MCP client sends:
//! a drawer chest from the template (slides included), feet from the
//! catalog, new models saved to the user catalog, and pictures.
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
    assert!(change.committed, "{:?}", change.summary);
    change.result
}

fn dump(name: &str, png: &[u8]) {
    if let Some(dir) = std::env::var_os("PMCAB_RENDER_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join(name), png).unwrap();
    }
}

fn temp() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pmcab-fittings-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn drawer_chest_on_feet_with_slides() {
    let dir = temp();
    let mut ws = workspace(&dir);
    let catalog = ws
        .list_hardware_catalog(input(json!({ "kind": "slide" })))
        .unwrap();
    let families: Vec<&str> = catalog["slides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["slide"].as_str().unwrap())
        .collect();
    assert!(families.contains(&"tt45-slowmotion") && families.contains(&"tt90-slow"));

    let generated = ws
        .generate_template(input(json!({
            "kind": "drawers", "name": "Chest",
            "dimensions": { "width": 600, "depth": 560, "height": 720, "side_clearance": 13 }
        })))
        .unwrap();
    let slides = generated["slides"].as_array().unwrap();
    assert_eq!(slides.len(), 3, "{generated:#}");
    assert!(slides.iter().all(|s| s["ok"] == true), "{slides:#?}");
    assert_eq!(slides[0]["product"], "0073.045500SX");
    assert!(
        generated["warnings"]
            .to_string()
            .contains("side_clearance is ignored")
    );

    let listed = ws
        .list_slides(input(json!({ "drawer": "Drawer 2" })))
        .unwrap();
    assert_eq!(listed["slides"].as_array().unwrap().len(), 1);
    assert_eq!(listed["slides"][0]["sides"][0]["gap_mm"], 12.7);

    // A shorter length on one drawer: still fits, still no issues.
    let updated = result(
        ws.update_slide(input(json!({ "slide": "Drawer 1", "length": 450 })))
            .unwrap(),
    );
    assert_eq!(updated["product"], "0073.045450SX");
    assert_eq!(updated["ok"], true);

    // Raise the chest and put four square chrome feet under its bottom.
    result(
        ws.transform_objects(input(
            json!({ "objects": ["Chest"], "translate_mm": [0, 0, 100] }),
        ))
        .unwrap(),
    );
    let feet = result(
        ws.add_foot(input(json!({
            "model": "generic-post-square-100",
            "parent": "Chest",
            "anchor": "top_center",
            "positions": [[40, 40, 100], [560, 40, 100], [40, 500, 100], [560, 500, 100]]
        })))
        .unwrap(),
    );
    let rows = feet["feet"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0]["mounting_face_centre"], json!([40.0, 40.0, 100.0]));
    assert_eq!(rows[0]["world"]["z"], 0.0);
    let listed = ws.list_feet().unwrap();
    assert_eq!(listed["per_model"]["generic-post-square-100"], 4);
    assert_eq!(listed["pinned"].as_array().unwrap().len(), 1);

    // The scene description sees the feet under the bottom and the slides.
    let scene = ws
        .describe_scene(input(json!({ "detail": "full" })))
        .unwrap();
    assert_eq!(scene["overlaps"], json!([]), "{:#}", scene["overlaps"]);
    let kinds: Vec<&str> = scene["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.iter().filter(|k| **k == "foot").count(), 4);
    assert_eq!(kinds.iter().filter(|k| **k == "slide").count(), 6);

    let picture = ws
        .render_drawer_opening(input(json!({ "drawer": "Drawer 2", "fraction": 0.8 })))
        .unwrap();
    assert_eq!(picture.json["extension_mm"], 400.0);
    dump("service-drawer-open.png", &picture.images[0]);

    // Deleting the chest removes its slides and feet in one step.
    let dry = ws
        .delete_object(input(json!({ "ref": "Chest", "dry_run": true })))
        .unwrap();
    assert!(!dry.committed);
    assert_eq!(dry.result["drawer_slides"], 3);
    assert_eq!(dry.result["hardware_items"], 4);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn agents_create_feet_and_slides_that_reach_the_catalog() {
    let dir = temp();
    let mut ws = workspace(&dir);
    ws.new_project(input(json!({ "name": "Bench" }))).unwrap();

    let created = result(
        ws.create_foot_model(input(json!({
            "name": "Pé cônico madeira 12 cm",
            "shape": { "kind": "tapered", "height": 120, "top": { "diameter": 45 }, "bottom": { "diameter": 28 } },
            "color": "#8a5a2b",
            "finish": "wood",
            "mounting_holes": [[0, 0]],
            "save_to_catalog": true
        })))
        .unwrap(),
    );
    assert_eq!(created["model"], "pe-conico-madeira-12-cm");
    assert_eq!(created["entry"]["trust"], "user_supplied");
    let file = created["saved_to"].as_str().unwrap();
    assert!(
        std::fs::read_to_string(file)
            .unwrap()
            .contains("pe-conico-madeira-12-cm")
    );

    // A later session sees it in the catalog.
    let mut later = workspace(&dir);
    let catalog = later
        .list_hardware_catalog(input(json!({ "kind": "foot", "reload": true })))
        .unwrap();
    assert!(
        catalog["feet"]
            .to_string()
            .contains("pe-conico-madeira-12-cm")
    );

    // Invalid shapes are refused with the field named.
    let bad = ws
        .create_foot_model(input(json!({
            "name": "Broken",
            "shape": { "kind": "post", "height": 100, "tube": { "diameter": 30 }, "plate": { "diameter": 60 } }
        })))
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::CatalogError);
    assert!(
        bad.message.contains("shape.plate_thickness"),
        "{}",
        bad.message
    );

    let placed = result(
        ws.add_foot(input(
            json!({ "model": "pe-conico-madeira-12-cm", "name": "Leg", "pose": { "x": 100 } }),
        ))
        .unwrap(),
    );
    assert_eq!(placed["feet"][0]["box_mm"], json!([45.0, 45.0, 120.0]));

    let slide = result(
        ws.create_slide_model(input(json!({
            "name": "Light slide",
            "height": 27,
            "clearance": { "nominal": 12.5, "plus": 0.5 },
            "lengths": [
                { "length": 300, "cabinet_holes": [35, 163] },
                { "length": 400, "travel": 380, "cabinet_holes": [35, 227] }
            ],
            "save_to_catalog": true
        })))
        .unwrap(),
    );
    assert_eq!(slide["entries"].as_array().unwrap().len(), 2);
    let text = std::fs::read_to_string(dir.join("catalogs").join("user-models.toml")).unwrap();
    assert!(text.contains("light-slide") && text.contains("pe-conico-madeira-12-cm"));

    // A clearance tolerance as large as the clearance is refused.
    let bad = ws
        .create_slide_model(input(json!({
            "name": "Bad", "height": 45, "clearance": { "nominal": 10, "minus": 10 },
            "lengths": [{ "length": 300 }]
        })))
        .unwrap_err();
    assert!(bad.message.contains("clearance"), "{}", bad.message);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn slides_on_a_hand_built_drawer_are_checked() {
    let dir = temp();
    let mut ws = workspace(&dir);
    ws.generate_template(input(json!({
        "kind": "drawers", "name": "Chest", "drawer_count": 2,
        "slides": { "none": true },
        "dimensions": { "side_clearance": 15 }
    })))
    .unwrap();
    assert!(
        ws.list_slides(input(json!({}))).unwrap()["slides"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // 15 mm gaps are outside TT45's 12.7 +0.5: the dry run explains.
    let dry = ws
        .add_slides(input(json!({ "drawer": "Drawer 1", "dry_run": true })))
        .unwrap();
    assert!(!dry.committed);
    assert_eq!(dry.result["ok"], false);
    assert!(dry.result["issues"].to_string().contains("gap"));
    // TT35 needs 12.7 too; there is no 15 mm family, so install anyway and see.
    let added = result(
        ws.add_slides(input(
            json!({ "drawer": "Drawer 1 left box side", "slide": "tt35-slowmotion" }),
        ))
        .unwrap(),
    );
    assert_eq!(added["issues"].as_array().unwrap().len(), 2);
    let again = ws
        .add_slides(input(json!({ "drawer": "Drawer 1" })))
        .unwrap_err();
    assert_eq!(again.code, ErrorCode::Conflict);
    result(
        ws.remove_slide(input(json!({ "slide": "Drawer 1" })))
            .unwrap(),
    );
    std::fs::remove_dir_all(&dir).ok();
}
