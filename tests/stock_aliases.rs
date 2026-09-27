use plan_my_cabinet::board_commands::{NewBoard, NewMaterial};
use plan_my_cabinet::commands::{EditError, ProjectEditor};
use plan_my_cabinet::domain::{
    BoardGrain, DomainError, Project, SrgbColor, StockGrain, StockSource,
};
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence::{PersistenceError, prepare_bytes, serialize};
use plan_my_cabinet::stock_commands::{StockError, StockInput};
use plan_my_cabinet::units::{Length, Pose, Quaternion};
use uuid::Uuid;

fn mm(value: i64) -> Length {
    Length::from_micrometres(value * 1000)
}

fn fixture() -> (ProjectEditor, StockInput) {
    let mut editor = ProjectEditor::new(Project::new("Stock", Currency::Brl)).unwrap();
    let material_id = editor
        .create_material(NewMaterial {
            name: "Birch".into(),
            thickness: mm(18),
            grain: BoardGrain::Unrestricted,
        })
        .unwrap();
    let input = StockInput {
        name: "Sheet".into(),
        material_id,
        length: mm(500),
        width: mm(300),
        thickness: mm(18),
        grain: StockGrain::Unknown,
        source: StockSource::ToPurchase,
        price: None,
        trim: [Length::ZERO; 4],
    };
    (editor, input)
}

fn label(editor: &ProjectEditor, id: Uuid) -> String {
    editor.project().stock_alias(id).unwrap().to_owned()
}

#[test]
fn quantity_duplicate_edit_reorder_and_save_preserve_identity_and_alias() {
    let (mut editor, mut input) = fixture();
    let material_id = input.material_id;
    let sheets = editor.create_stock(input.clone(), 3).unwrap();
    assert_eq!(
        sheets
            .iter()
            .map(|id| label(&editor, *id))
            .collect::<Vec<_>>(),
        ["S1", "S2", "S3"]
    );
    let board = editor
        .create_board(NewBoard {
            name: "Side".into(),
            material_id: input.material_id,
            length: mm(500),
            width: mm(300),
            pose: Pose::new([0.0; 3], Quaternion::IDENTITY).unwrap(),
        })
        .unwrap();
    let allocated = editor
        .project()
        .allocations
        .iter()
        .find(|a| a.board_id == board)
        .unwrap()
        .stock_id;
    assert_eq!(allocated, sheets[0]);
    assert_eq!(
        editor.delete_stock(allocated),
        Err(EditError::Command(StockError::AllocatedStock(allocated)))
    );
    input.source = StockSource::Owned;
    let owned = editor.create_stock(input.clone(), 2).unwrap();
    assert_eq!(label(&editor, owned[0]), "O1");
    assert_eq!(label(&editor, owned[1]), "O2");

    input.name = "Changed owner and name".into();
    editor.edit_stock(sheets[2], input).unwrap();
    editor.reorder_stock(sheets[2], 0).unwrap();
    assert_eq!(editor.project().ordered_stock()[0].id, sheets[2]);
    assert_eq!(label(&editor, sheets[2]), "S3");
    assert_eq!(
        editor
            .project()
            .stock
            .iter()
            .find(|p| p.id == sheets[2])
            .unwrap()
            .source,
        StockSource::Owned
    );
    let copy = editor.duplicate_stock(sheets[2]).unwrap();
    assert_ne!(copy, sheets[2]);
    assert_eq!(label(&editor, copy), "O3");
    editor.undo().unwrap();
    assert!(editor.project().stock_alias(copy).is_none());
    editor.redo().unwrap();
    assert_eq!(label(&editor, copy), "O3");
    editor
        .set_material_color(material_id, Some(SrgbColor([20, 80, 130])))
        .unwrap();
    assert_eq!(label(&editor, sheets[2]), "S3");

    let reopened = prepare_bytes(&serialize(editor.project()).unwrap())
        .unwrap()
        .into_editor();
    assert_eq!(
        reopened.project().stock_aliases,
        editor.project().stock_aliases
    );
    assert_eq!(
        reopened.project().material_colors,
        editor.project().material_colors
    );
    assert_eq!(reopened.project().next_stock_s_alias, 4);
    assert_eq!(reopened.project().next_stock_o_alias, 4);
    assert_eq!(reopened.project().ordered_stock()[0].id, sheets[2]);
    assert_eq!(
        reopened
            .project()
            .allocations
            .iter()
            .find(|a| a.board_id == board)
            .unwrap()
            .stock_id,
        allocated
    );
}

#[test]
fn deletion_and_undo_branches_never_reuse_reserved_numbers() {
    let (mut editor, mut input) = fixture();
    input.source = StockSource::Owned;
    let first = editor.create_stock(input.clone(), 2).unwrap();
    editor.delete_stock(first[1]).unwrap();
    let newer = editor.create_stock(input.clone(), 1).unwrap()[0];
    assert_eq!(label(&editor, newer), "O3");
    editor.undo().unwrap(); // undo creation, retain O3 in counter
    editor.undo().unwrap(); // undo deletion, restore O2
    assert_eq!(label(&editor, first[1]), "O2");
    assert_eq!(editor.project().next_stock_o_alias, 4);
    let branched = editor.create_stock(input.clone(), 1).unwrap()[0];
    assert_eq!(label(&editor, branched), "O4");
    editor.undo().unwrap();
    assert_eq!(editor.project().next_stock_o_alias, 5);
    let mut reopened = prepare_bytes(&serialize(editor.project()).unwrap())
        .unwrap()
        .into_editor();
    assert_eq!(reopened.project().next_stock_o_alias, 5);
    assert_eq!(reopened.project().stock_alias(first[1]), Some("O2"));
    let next = reopened.create_stock(input, 1).unwrap()[0];
    assert_eq!(label(&reopened, next), "O5");
}

#[test]
fn legacy_priority_and_uuid_tie_break_are_deterministic_without_dirtying() {
    let (mut editor, input) = fixture();
    let purchased = editor.create_stock(input.clone(), 3).unwrap();
    let owned = editor
        .create_stock(
            StockInput {
                source: StockSource::Owned,
                ..input
            },
            2,
        )
        .unwrap();
    let mut value = serde_json::to_value(editor.project()).unwrap();
    value["schema_version"] = 1.into();
    value.as_object_mut().unwrap().remove("stock_aliases");
    value.as_object_mut().unwrap().remove("next_stock_s_alias");
    value.as_object_mut().unwrap().remove("next_stock_o_alias");
    let priorities = [
        (purchased[0], 8),
        (purchased[1], 1),
        (purchased[2], 1),
        (owned[0], 1),
        (owned[1], 0),
    ];
    for piece in value["stock"].as_array_mut().unwrap() {
        let id: Uuid = piece["id"].as_str().unwrap().parse().unwrap();
        piece["priority"] = priorities
            .iter()
            .find(|(key, _)| *key == id)
            .unwrap()
            .1
            .into();
    }
    value["stock"].as_array_mut().unwrap().reverse();
    let source = serde_json::to_vec(&value).unwrap();
    let migrated = prepare_bytes(&source).unwrap().into_editor();
    let mut expected = priorities.to_vec();
    expected.sort_by_key(|(id, priority)| (*priority, *id));
    let mut s = 1;
    let mut o = 1;
    for (id, _) in expected {
        let expected_alias = if purchased.contains(&id) {
            let alias = format!("S{s}");
            s += 1;
            alias
        } else {
            let alias = format!("O{o}");
            o += 1;
            alias
        };
        assert_eq!(
            migrated.project().stock_alias(id),
            Some(expected_alias.as_str())
        );
    }
    assert!(!migrated.is_dirty());
    assert!(!migrated.can_undo());
    assert_eq!(migrated.project().revision, editor.project().revision);
    assert_eq!(
        prepare_bytes(&source).unwrap().project().stock_aliases,
        migrated.project().stock_aliases
    );
    let saved = serialize(migrated.project()).unwrap();
    assert_eq!(
        prepare_bytes(&saved).unwrap().project().stock_aliases,
        migrated.project().stock_aliases
    );
    assert_eq!(source, serde_json::to_vec(&value).unwrap());
    assert_eq!(migrated.project().stock_alias(owned[1]), Some("O1"));
}

#[test]
fn malformed_aliases_and_counters_are_rejected_without_repair() {
    let (mut editor, input) = fixture();
    editor.create_stock(input, 2).unwrap();
    let valid = serde_json::to_value(editor.project()).unwrap();
    let keys: Vec<_> = valid["stock_aliases"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    for alias in ["O0", "S01", "S0", "S999", "X1", "S1extra", "", "é1"] {
        let mut bad = valid.clone();
        bad["stock_aliases"][&keys[0]] = alias.into();
        assert!(matches!(
            prepare_bytes(&serde_json::to_vec(&bad).unwrap()),
            Err(PersistenceError::InvalidProject(
                DomainError::InvalidStockAlias
            ))
        ));
    }
    let mut duplicate = valid.clone();
    duplicate["stock_aliases"][&keys[0]] = duplicate["stock_aliases"][&keys[1]].clone();
    assert!(matches!(
        prepare_bytes(&serde_json::to_vec(&duplicate).unwrap()),
        Err(PersistenceError::InvalidProject(
            DomainError::InvalidStockAlias
        ))
    ));
    let mut partial = valid.clone();
    partial["stock_aliases"]
        .as_object_mut()
        .unwrap()
        .remove(&keys[0]);
    assert!(matches!(
        prepare_bytes(&serde_json::to_vec(&partial).unwrap()),
        Err(PersistenceError::InvalidProject(
            DomainError::InvalidStockAlias
        ))
    ));
    let mut counter = valid;
    counter["next_stock_s_alias"] = 2.into();
    assert!(matches!(
        prepare_bytes(&serde_json::to_vec(&counter).unwrap()),
        Err(PersistenceError::InvalidProject(
            DomainError::InvalidStockAlias
        ))
    ));
    let id = editor.project().stock[0].id;
    assert_eq!(
        editor.transact(|p| -> Result<(), ()> {
            p.stock_aliases.insert(id, "O17".into());
            Ok(())
        }),
        Err(EditError::InvalidProject(DomainError::InvalidStockAlias))
    );
    assert_eq!(editor.project().stock_alias(id), Some("S1"));
    let unknown = Uuid::new_v4();
    assert_eq!(
        editor.delete_stock(unknown),
        Err(EditError::Command(StockError::MissingStock(unknown)))
    );
}
