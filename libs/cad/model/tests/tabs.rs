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
        instances: vec![Instance { id: 1, studio: 0, part, name: "Caja".into(), position: [1.0, 2.0, 3.0], rotation: [0.0, 0.1, 0.0], fixed: true }],
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

#[test]
fn tab_feature_roundtrip() {
    for (tab, f) in [(0, 0), (0, 7), (3, 12), (4095, (1 << 20) - 1)] {
        assert_eq!(split_tab_feature(tab_feature(tab, FeatureId(f))), (tab, FeatureId(f)));
    }
    assert_eq!(tab_feature(0, FeatureId(5)), FeatureId(5), "la pestaña 0 no cambia los números");
}

#[test]
fn legacy_assembly_moves_to_its_own_tab() {
    let mut tabs = Tabs::from_legacy(Some(legacy_doc()));
    let other = tabs.add_design(None);
    assert_eq!(tabs.lift_assemblies(), 1);
    let ids: Vec<_> = tabs.tabs.iter().map(|t| (t.id, t.name.as_str(), t.kind.is_design())).collect();
    assert_eq!(ids, [(0, "Diseño 1", true), (2, "Ensamble 1", false), (other, "Diseño 2", true)], "justo después de su diseño");
    assert!(tabs.design(0).unwrap().assembly.is_none(), "el diseño queda sin ensamble");
    let TabKind::Assembly { assembly } = &tabs.get(2).unwrap().kind else { panic!() };
    assert_eq!(assembly.instances[0].studio, 0);
    assert_eq!(assembly.instances[0].position, [1.0, 2.0, 3.0]);
    assert_eq!(tabs.users(0), [2]);
    assert!(tabs.users(other).is_empty());
    // Otra vez: nada (y un ensamble vacío dentro de un diseño no hace pestaña)
    tabs.design_mut(other).unwrap().assembly = Some(Assembly::default());
    assert_eq!(tabs.lift_assemblies(), 0);
    assert!(tabs.design(other).unwrap().assembly.is_none());
    assert_eq!(tabs.tabs.len(), 3);
}

#[test]
fn flat_assembly_roundtrip() {
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let b = tabs.add_design(None);
    let asm = tabs.add_assembly(None);
    let part = |studio, f| (studio, PartId { feature: FeatureId(f), index: 1 });
    let inst = |id, (studio, part)| Instance { id, studio, part, name: format!("I{id}"), position: [0.0; 3], rotation: [0.0; 3], fixed: id == 1 };
    let assembly = Assembly { instances: vec![inst(1, part(0, 3)), inst(2, part(b, 3))], next_id: 3, ..Default::default() };
    let TabKind::Assembly { assembly: stored } = &mut tabs.get_mut(asm).unwrap().kind else { panic!() };
    *stored = assembly.clone();
    let flat = tabs.flat_assembly(asm).unwrap();
    assert_eq!(flat.instances[0].part.feature, FeatureId(3));
    assert_eq!(flat.instances[1].part.feature, tab_feature(b, FeatureId(3)));
    assert_ne!(flat.instances[0].part, flat.instances[1].part, "misma operación en dos pestañas: piezas distintas");
    tabs.set_flat_assembly(asm, flat).unwrap();
    let TabKind::Assembly { assembly: back } = &tabs.get(asm).unwrap().kind else { panic!() };
    assert_eq!(*back, assembly);
    assert!(tabs.flat_assembly(0).is_none());
    assert!(tabs.set_flat_assembly(0, Assembly::default()).is_err());
}

#[test]
fn flat_part_props_carry_names_and_materials() {
    let mut a = box_doc(10.0);
    a.material = Some(Material { name: "PLA".into(), density: 1.24 });
    let pa = PartId { feature: a.features[0].id, index: 0 };
    a.parts.push(PartProps { part: pa, name: Some("Caja".into()), color: Some("#ff0000".into()), hidden: false, material: None });
    let mut tabs = Tabs::from_legacy(Some(a));
    let b = tabs.add(TabKind::Design { document: Box::new(box_doc(20.0)) }, None);
    let pb = PartId { feature: tab_feature(b, FeatureId(0)), index: 0 };
    let (props, fabrication) = tabs.flat_part_props(&[(pb, "Pieza 1".into())]);
    assert!(fabrication.is_none());
    assert_eq!(props.len(), 2);
    assert_eq!((props[0].name.as_deref(), props[0].material.as_ref().map(|m| m.name.as_str())), (Some("Caja"), Some("PLA")), "el material del diseño");
    assert_eq!((props[1].part, props[1].name.as_deref()), (pb, Some("Pieza 1")));
}

#[test]
fn merged_evaluation_keeps_parts_apart() {
    if !occt::available() {
        return;
    }
    let (a, b) = (box_doc(10.0), box_doc(40.0));
    let (ea, eb) = (a.evaluate(), b.evaluate());
    let merged = merge_evaluations([(0, &ea), (3, &eb)]);
    assert_eq!(merged.parts.len(), 2);
    assert_eq!(merged.parts[0].id, ea.parts[0].id);
    assert_eq!(merged.parts[1].id.feature, tab_feature(3, eb.parts[0].id.feature));
    let v = merged.parts[1].shape.mass().unwrap().volume;
    assert!((v - 24000.0).abs() < 1e-6, "{v}");
}
