//! Edge banding through the agent API: the template comes out banded,
//! boards report their edges, and banding tools edit in one undo step.
use plan_my_cabinet::service::dto::Change;
use plan_my_cabinet::service::{ErrorCode, Workspace, WorkspaceConfig};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn input<T: DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("tool input deserializes")
}

fn result(change: Change<Value>) -> Value {
    assert!(change.committed);
    change.result
}

fn board(ws: &Workspace, name: &str) -> Value {
    ws.get_board(input(json!({ "ref": name }))).unwrap()
}

#[test]
fn templates_band_free_edges_and_tools_change_them() {
    let dir = std::env::temp_dir().join(format!("pmcab-banding-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = Workspace::new(WorkspaceConfig {
        catalog_dir: Some(dir.join("catalogs")),
        user_data_dir: Some(dir.clone()),
        ..Default::default()
    });
    ws.generate_template(input(json!({
        "kind": "base",
        "name": "Base",
        "dimensions": { "width": 600, "depth": 560, "height": 720 }
    })))
    .unwrap();

    let bands = ws.list_edge_bands().unwrap();
    let band = bands["edge_bands"][0].clone();
    assert_eq!(band["name"], "White band 1x22", "{bands:#}");
    assert!(band["banded_metres"].as_f64().unwrap() > 1.0, "{bands:#}");

    let side = board(&ws, "Left side");
    let edges = &side["banding"]["edges"];
    assert_eq!(edges["length_1"]["band"], "White band 1x22", "{side:#}");
    assert_eq!(edges["length_1"]["front"], true);
    assert_eq!(edges["length_2"]["joined"], true);
    assert_eq!(edges["length_2"]["touching"]["board"], "Overlay back");
    assert_eq!(board(&ws, "Overlay back")["banding"]["accepted"], false);

    // Edges with a value: the front edge off, the rear forced on.
    let revision = ws.get_project().unwrap().revision;
    let changed = result(
        ws.set_board_banding(input(json!({
            "boards": ["Left side", "Right side", "Overlay back"],
            "edges": ["front"],
            "value": "off"
        })))
        .unwrap(),
    );
    assert_eq!(changed["skipped"], json!(["Overlay back"]));
    assert_eq!(ws.get_project().unwrap().revision, revision + 1);
    let side = board(&ws, "Right side");
    assert_eq!(side["banding"]["edges"]["length_1"]["band"], Value::Null);
    assert_eq!(side["banding"]["edges"]["length_1"]["setting"], "off");

    // A preset, then a new band made a material's default.
    result(
        ws.set_board_banding(input(json!({ "boards": ["Bottom"], "preset": "all_four" })))
            .unwrap(),
    );
    let bottom = board(&ws, "Bottom");
    for edge in ["length_1", "length_2", "width_1", "width_2"] {
        assert_eq!(
            bottom["banding"]["edges"][edge]["setting"], "on",
            "{bottom:#}"
        );
    }
    result(
        ws.create_edge_band(input(json!({
            "name": "Fita Preta 1x22", "thickness": 1, "height": 22, "color": "#202020"
        })))
        .unwrap(),
    );
    let materials = ws.list_materials().unwrap();
    for material in materials["materials"].as_array().unwrap() {
        if material["name"] == "White MDF" {
            result(
                ws.update_material(input(json!({
                    "material": material["id"], "default_band": "Fita Preta 1x22"
                })))
                .unwrap(),
            );
        }
    }
    let side = board(&ws, "Left side");
    assert_eq!(
        side["banding"]["edges"]["width_2"]["band"],
        "Fita Preta 1x22"
    );
    assert!(
        materials["materials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["kind"] == "hdf" && m["takes_banding"] == false)
    );

    // Bands in use stay; errors say why.
    let refused = ws
        .remove_edge_band(input(json!({ "band": "Fita Preta 1x22" })))
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Conflict);
    let refused = ws
        .set_board_banding(input(
            json!({ "boards": ["Overlay back"], "preset": "all_four" }),
        ))
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::InvalidArgument);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_part_list_previews_and_writes_a_cortecloud_file() {
    let dir = std::env::temp_dir().join(format!("pmcab-export-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = Workspace::new(WorkspaceConfig {
        catalog_dir: Some(dir.join("catalogs")),
        user_data_dir: Some(dir.clone()),
        ..Default::default()
    });
    ws.generate_template(input(json!({
        "kind": "base",
        "name": "Base",
        "dimensions": { "width": 600, "depth": 560, "height": 720 }
    })))
    .unwrap();
    let preview = ws.get_part_list(input(json!({}))).unwrap();
    let parts = preview["parts"].as_array().unwrap();
    assert!(!parts.is_empty(), "{preview:#}");
    let side = parts.iter().find(|p| p["name"] == "Left side").unwrap();
    assert_eq!(side["cabinet"], "Base");
    assert_eq!(side["banding"]["length_1"], "White band 1x22");
    assert_eq!(
        preview["file_preview"]["parts"].as_array().unwrap().len(),
        parts.len()
    );

    let bare = ws
        .get_part_list(input(json!({ "include": { "banding": false } })))
        .unwrap();
    assert!(
        bare["parts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["banding"] == json!({}))
    );
    let path = dir.join("base-cortecloud.json");
    let revision = ws.get_project().unwrap().revision;
    let written = ws
        .export_design(input(json!({ "path": path.to_str().unwrap() })))
        .unwrap();
    assert!(
        written.committed && !written.changed,
        "a receipt is not an edit"
    );
    assert_eq!(ws.get_project().unwrap().revision, revision);
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file["parts"].as_array().unwrap().len(), parts.len());
    let refused = ws
        .export_design(input(json!({ "path": path.to_str().unwrap() })))
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Conflict);
    ws.export_design(input(
        json!({ "path": path.to_str().unwrap(), "overwrite": true }),
    ))
    .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
