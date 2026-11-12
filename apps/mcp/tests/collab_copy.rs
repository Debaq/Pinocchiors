//! La copia del proyecto que el anfitrión manda a la sala: exportar sin marcar
//! el proyecto como guardado y abrirla en otro estado (el del invitado).

use pinocchio_app_lib::cad::bridge::{try_dispatch, Reply};
use pinocchio_app_lib::project::{export_copy_impl, open_project_impl};
use pinocchio_app_lib::state::AppState;
use serde_json::{json, Value};

fn json_of(state: &AppState, cmd: &str, args: Value) -> Value {
    match try_dispatch(state, cmd, &args).expect("comando conocido").expect("sin error") {
        Reply::Json(v) => v,
        Reply::Bytes(_) => panic!("se esperaba JSON"),
    }
}

#[test]
fn copia_del_anfitrion_llega_igual_al_invitado() {
    let host = AppState::new();
    json_of(&host, "cad_new", json!({}));
    json_of(&host, "cad_tab_add", json!({ "kind": "design", "name": "Soporte" }));
    let tabs_host = json_of(&host, "cad_tabs", json!({}));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copia.pinocchio");
    let size = export_copy_impl(&host, path.to_str().unwrap(), r#"{"ui":1}"#, None).unwrap();
    assert!(size > 0);

    let guest = AppState::new();
    let opened = open_project_impl(&guest, path.to_str().unwrap()).unwrap();
    assert_eq!(opened.ui, r#"{"ui":1}"#);
    assert_eq!(json_of(&guest, "cad_tabs", json!({})), tabs_host);

    // La copia no marcó el proyecto del anfitrión como guardado
    assert!(host.last_saved_hash.lock().unwrap().is_none());
}
