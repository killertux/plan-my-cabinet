use plan_my_cabinet::commands::{EditError, ProjectEditor};
use plan_my_cabinet::domain::{DomainError, Project};
use plan_my_cabinet::export::fingerprint;
use plan_my_cabinet::money::Currency;
use plan_my_cabinet::persistence::{PersistenceError, prepare_bytes, serialize};
use plan_my_cabinet::units::{Length, UnitError};

const FIRST_DATE: u64 = 1_780_000_000_000;
const SECOND_DATE: u64 = FIRST_DATE + 86_400_000;

fn mm(n: i64) -> Length {
    Length::from_micrometres(n * 1000)
}

fn editor() -> ProjectEditor {
    ProjectEditor::new(Project::new("Kerf provenance", Currency::Brl)).unwrap()
}

#[test]
fn explicit_confirmation_records_date_and_reconfirmation_updates_it_without_changing_cut_inputs() {
    let mut editor = editor();
    let before = fingerprint(editor.project());
    assert!(editor.confirm_shop_kerf_at(FIRST_DATE).unwrap());
    assert_eq!(editor.project().confirmed_shop_kerf, Some(mm(5)));
    assert_eq!(
        editor.project().confirmed_shop_kerf_unix_ms,
        Some(FIRST_DATE)
    );
    let confirmed = fingerprint(editor.project());
    assert_eq!(confirmed.wood, before.wood);
    assert_ne!(confirmed.packet, before.packet); // The draft warning changed.
    assert!(!editor.confirm_shop_kerf_at(FIRST_DATE).unwrap());
    assert!(editor.confirm_shop_kerf_at(SECOND_DATE).unwrap());
    assert_eq!(
        editor.project().confirmed_shop_kerf_unix_ms,
        Some(SECOND_DATE)
    );
    assert_eq!(fingerprint(editor.project()), confirmed); // Date alone is not printed.
    let reopened = prepare_bytes(&serialize(editor.project()).unwrap()).unwrap();
    assert_eq!(reopened.project().confirmed_shop_kerf, Some(mm(5)));
    assert_eq!(
        reopened.project().confirmed_shop_kerf_unix_ms,
        Some(SECOND_DATE)
    );
    assert!(!reopened.into_editor().is_dirty());
}

#[test]
fn kerf_change_clears_pair_and_undo_redo_restore_the_corresponding_pair() {
    let mut editor = editor();
    editor.confirm_shop_kerf_at(FIRST_DATE).unwrap();
    editor.set_cutting_kerf(mm(6)).unwrap();
    assert_eq!(editor.project().confirmed_shop_kerf, None);
    assert_eq!(editor.project().confirmed_shop_kerf_unix_ms, None);
    editor.confirm_shop_kerf_at(SECOND_DATE).unwrap();
    assert_eq!(editor.project().confirmed_shop_kerf, Some(mm(6)));
    assert_eq!(
        editor.project().confirmed_shop_kerf_unix_ms,
        Some(SECOND_DATE)
    );
    editor.undo().unwrap();
    assert_eq!(editor.project().cutting_kerf, mm(6));
    assert_eq!(editor.project().confirmed_shop_kerf_unix_ms, None);
    editor.undo().unwrap();
    assert_eq!(editor.project().cutting_kerf, mm(5));
    assert_eq!(editor.project().confirmed_shop_kerf, Some(mm(5)));
    assert_eq!(
        editor.project().confirmed_shop_kerf_unix_ms,
        Some(FIRST_DATE)
    );
    editor.redo().unwrap();
    assert_eq!(editor.project().confirmed_shop_kerf, None);
    editor.redo().unwrap();
    assert_eq!(editor.project().confirmed_shop_kerf, Some(mm(6)));
    assert_eq!(
        editor.project().confirmed_shop_kerf_unix_ms,
        Some(SECOND_DATE)
    );
    // Generic transactions also cannot accidentally carry an old date to a new kerf.
    editor
        .transact(|p| {
            p.cutting_kerf = mm(7);
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(editor.project().confirmed_shop_kerf, None);
    assert_eq!(editor.project().confirmed_shop_kerf_unix_ms, None);
    assert_eq!(editor.project().cutting_kerf, mm(7));
}

#[test]
fn legacy_confirmed_kerf_has_unknown_date_until_explicit_reconfirmation() {
    let mut value = serde_json::to_value(Project::new("Legacy", Currency::Brl)).unwrap();
    value["schema_version"] = 1.into();
    value["confirmed_shop_kerf"] = value["cutting_kerf"].clone();
    let bytes = serde_json::to_vec(&value).unwrap();
    let mut editor = prepare_bytes(&bytes).unwrap().into_editor();
    assert_eq!(editor.project().confirmed_shop_kerf, Some(mm(5)));
    assert_eq!(editor.project().confirmed_shop_kerf_unix_ms, None);
    assert!(!editor.is_dirty());
    assert!(!editor.can_undo());
    let serialized = serialize(editor.project()).unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&serialized)
            .unwrap()
            .get("confirmed_shop_kerf_unix_ms")
            .is_none()
    );
    assert_eq!(
        prepare_bytes(&serialized)
            .unwrap()
            .project()
            .confirmed_shop_kerf_unix_ms,
        None
    );
    editor.confirm_shop_kerf_at(FIRST_DATE).unwrap();
    assert_eq!(
        editor.project().confirmed_shop_kerf_unix_ms,
        Some(FIRST_DATE)
    );
}

#[test]
fn malformed_or_orphan_dates_and_mismatched_values_are_rejected() {
    let mut editor = editor();
    let original = editor.project().clone();
    let mut value = serde_json::to_value(&original).unwrap();
    value["confirmed_shop_kerf_unix_ms"] = FIRST_DATE.into();
    assert!(matches!(
        prepare_bytes(&serde_json::to_vec(&value).unwrap()),
        Err(PersistenceError::InvalidProject(
            DomainError::InvalidKerfConfirmationDate
        ))
    ));
    value["confirmed_shop_kerf"] = value["cutting_kerf"].clone();
    for invalid in [
        serde_json::json!(-1),
        serde_json::json!("2026-09-26"),
        serde_json::json!(253_402_300_800_000_u64),
    ] {
        value["confirmed_shop_kerf_unix_ms"] = invalid;
        assert!(prepare_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    value["confirmed_shop_kerf_unix_ms"] = FIRST_DATE.into();
    value["confirmed_shop_kerf"] = serde_json::to_value(mm(6)).unwrap();
    assert!(matches!(
        prepare_bytes(&serde_json::to_vec(&value).unwrap()),
        Err(PersistenceError::InvalidProject(
            DomainError::InvalidCuttingKerf(UnitError::InvalidNumber)
        ))
    ));
    assert_eq!(editor.project(), &original);
    assert_eq!(
        editor.confirm_shop_kerf_at(253_402_300_800_000),
        Err(EditError::InvalidProject(
            DomainError::InvalidKerfConfirmationDate
        ))
    );
    assert_eq!(editor.project(), &original);
}
