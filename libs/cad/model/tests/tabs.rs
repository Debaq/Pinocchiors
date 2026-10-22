use cad_model::*;

fn box_doc(dx: f64) -> Document {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx, dy: 20.0, dz: 30.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    doc
}

/// Un diseño de antes, con un STEP importado (bytes) y un ensamble adentro
fn legacy_doc() -> Document {
    let mut doc = box_doc(10.0);
    doc.add(FeatureKind::Import { format: ImportFormat::Step, data: vec![0, 1, 2, 255], op: BodyOp::Cut });
    doc.parameters.push(Parameter { name: "largo".into(), expr: "10".into() });
    let part = PartId { feature: doc.features[0].id, index: 0 };
    doc.assembly = Some(Assembly {
        instances: vec![Instance { id: 1, part, name: "Caja".into(), position: [1.0, 2.0, 3.0], rotation: [0.0, 0.1, 0.0], fixed: true }],
        next_id: 2,
        ..Default::default()
    });
    doc
}

#[test]
fn legacy_design_becomes_tab_zero_untouched() {
    let tabs = Tabs::from_legacy(Some(legacy_doc()));
    assert_eq!(tabs.tabs.len(), 1);
    assert_eq!((tabs.tabs[0].id, tabs.tabs[0].name.as_str(), tabs.active), (0, "Diseño 1", 0));
    assert_eq!(tabs.active_design(), Some(&legacy_doc()), "el diseño queda igual, ensamble incluido");
    assert!(Tabs::from_legacy(None).is_empty());
    assert!(Tabs::from_legacy(None).active_design().is_none());
}

#[test]
fn roundtrip_json_and_messagepack() {
    let mut tabs = Tabs::from_legacy(Some(legacy_doc()));
    tabs.add(TabKind::Design { document: Box::new(box_doc(40.0)) }, Some("Tapa".into()));
    tabs.add_assembly(None);
    let json: Tabs = serde_json::from_str(&serde_json::to_string(&tabs).unwrap()).unwrap();
    assert_eq!(json, tabs);
    let msgpack: Tabs = rmp_serde::from_slice(&rmp_serde::to_vec_named(&tabs).unwrap()).unwrap();
    assert_eq!(msgpack, tabs);
    // Forma del JSON que ve la interfaz
    let v = serde_json::to_value(&tabs).unwrap();
    assert_eq!(v["tabs"][1]["kind"]["type"], "design");
    assert_eq!(v["tabs"][2]["kind"]["type"], "assembly");
    assert_eq!(v["tabs"][2]["name"], "Ensamble 1");
}

#[test]
fn add_names_and_ids() {
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let a = tabs.add_design(None);
    assert_eq!((a, tabs.get(a).unwrap().name.as_str(), tabs.active), (1, "Diseño 2", 1));
    // Nombre repetido (sin importar mayúsculas): se numera
    let b = tabs.add_design(Some(" diseño 2 ".into()));
    assert_eq!(tabs.get(b).unwrap().name, "diseño 2 1");
    let c = tabs.add_assembly(Some("Motor".into()));
    assert_eq!(tabs.get(c).unwrap().name, "Motor");
    assert!(tabs.design(c).is_none(), "un ensamble no es un diseño");
    // Los ids no se reusan aunque se borre la última
    tabs.remove(c).unwrap();
    assert_eq!(tabs.add_design(None), c + 1);
    // Proyecto vacío: la primera pestaña es la 0
    let mut empty = Tabs::default();
    assert_eq!(empty.add_design(None), 0);
    assert_eq!(empty.tabs[0].name, "Diseño 1");
}

#[test]
fn rename_checks_names() {
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let a = tabs.add_design(None);
    assert!(tabs.rename(a, "   ").is_err());
    assert!(tabs.rename(a, "DISEÑO 1").is_err(), "ya existe");
    tabs.rename(a, "  Tapa ").unwrap();
    assert_eq!(tabs.get(a).unwrap().name, "Tapa");
    tabs.rename(a, "tapa").unwrap();
    assert!(tabs.rename(99, "x").is_err());
}

#[test]
fn duplicate_copies_next_to_the_original() {
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let last = tabs.add_design(None);
    let copy = tabs.duplicate(0).unwrap();
    assert_eq!(tabs.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [0, copy, last]);
    assert_eq!(tabs.get(copy).unwrap().name, "Diseño 1 (copia)");
    assert_eq!(tabs.active, copy);
    assert_eq!(tabs.design(copy), tabs.design(0));
    // La copia es independiente
    *tabs.design_mut(copy).unwrap() = box_doc(99.0);
    assert_eq!(tabs.design(0), Some(&box_doc(10.0)));
    let again = tabs.duplicate(0).unwrap();
    assert_eq!(tabs.get(again).unwrap().name, "Diseño 1 (copia) 1");
}

#[test]
fn remove_moves_the_active_tab() {
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let a = tabs.add_design(None);
    let b = tabs.add_assembly(None);
    tabs.activate(a).unwrap();
    assert_eq!(tabs.remove(a).unwrap().id, a);
    assert_eq!(tabs.active, b, "pasa a la de al lado");
    tabs.remove(b).unwrap();
    assert_eq!(tabs.active, 0);
    assert!(tabs.remove(0).is_err(), "la única no se borra");
    assert!(tabs.remove(7).is_err());
    assert!(tabs.activate(7).is_err());
}

#[test]
fn move_reorders() {
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let a = tabs.add_design(None);
    let b = tabs.add_design(None);
    tabs.move_to(b, 0).unwrap();
    assert_eq!(tabs.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [b, 0, a]);
    tabs.move_to(b, 99).unwrap();
    assert_eq!(tabs.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [0, a, b]);
    assert_eq!(tabs.active, b, "mover no cambia la activa");
}
