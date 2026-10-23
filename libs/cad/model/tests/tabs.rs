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
        instances: vec![Instance { id: 1, studio: 0, configuration: None, part, name: "Caja".into(), position: [1.0, 2.0, 3.0], rotation: [0.0, 0.1, 0.0], fixed: true }],
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
    // Un ensamble con su plano
    let with_sheet = tabs.add_assembly(None);
    let sheets = serde_json::json!({ "sheets": [{ "id": 1, "name": "Plano 1", "bom": true, "scale": 0.5 }], "next_id": 2 });
    tabs.set_assembly_drawing(with_sheet, Some(sheets.clone())).unwrap();
    assert!(tabs.set_assembly_drawing(0, None).is_err(), "un diseño guarda su plano en su documento");
    assert_eq!(tabs.assembly_document(with_sheet, &[]).unwrap().drawing, Some(sheets));
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
    let TabKind::Assembly { assembly, .. } = &tabs.get(2).unwrap().kind else { panic!() };
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
    let inst = |id, (studio, part)| Instance { id, studio, configuration: None, part, name: format!("I{id}"), position: [0.0; 3], rotation: [0.0; 3], fixed: id == 1 };
    let assembly = Assembly { instances: vec![inst(1, part(0, 3)), inst(2, part(b, 3))], next_id: 3, ..Default::default() };
    let TabKind::Assembly { assembly: stored, .. } = &mut tabs.get_mut(asm).unwrap().kind else { panic!() };
    *stored = assembly.clone();
    let flat = tabs.flat_assembly(asm).unwrap();
    assert_eq!(flat.instances[0].part.feature, FeatureId(3));
    assert_eq!(flat.instances[1].part.feature, tab_feature(b, FeatureId(3)));
    assert_ne!(flat.instances[0].part, flat.instances[1].part, "misma operación en dos pestañas: piezas distintas");
    tabs.set_flat_assembly(asm, flat).unwrap();
    let TabKind::Assembly { assembly: back, .. } = &tabs.get(asm).unwrap().kind else { panic!() };
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
    let (props, fabrication) = tabs.flat_part_props(0, &[(pb, "Pieza 1".into())]);
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

fn derived(studio: TabId, op: BodyOp) -> FeatureKind {
    FeatureKind::Derived(Derived { studio, parts: vec![], configuration: None, op, source: Default::default() })
}

/// Recalcula la pestaña `tab` con sus piezas derivadas llenas
fn eval_tab(tabs: &Tabs, tab: TabId) -> Evaluation {
    let mut doc = tabs.design(tab).unwrap().clone();
    tabs.fill_derived(tab, &mut doc, &mut |d| d.evaluate());
    doc.evaluate()
}

fn volume(ev: &Evaluation) -> f64 {
    ev.parts.iter().map(|p| p.shape.mass().unwrap().volume).sum()
}

#[test]
fn derived_is_saved_without_its_shapes() {
    let mut doc = Document::new();
    doc.add(derived(3, BodyOp::Cut));
    let json = serde_json::to_string(&doc).unwrap();
    assert!(!json.contains("source"), "{json}");
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc);
    // Sin `op` guardado: pieza aparte
    let FeatureKind::Derived(d) = serde_json::from_str::<FeatureKind>(r#"{"type":"derived","studio":1}"#).unwrap() else { panic!() };
    assert_eq!((d.op, d.parts.len()), (BodyOp::New, 0));
}

#[test]
fn derived_mold_follows_the_original() {
    if !occt::available() {
        return;
    }
    // Pestaña 0: una caja de 10×20×30. Pestaña 1: un bloque de 40×20×30 menos la caja (molde)
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let mold = tabs.add(TabKind::Design { document: Box::new(box_doc(40.0)) }, Some("Molde".into()));
    tabs.design_mut(mold).unwrap().add(derived(0, BodyOp::Cut));
    let ev = eval_tab(&tabs, mold);
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert!((volume(&ev) - 30.0 * 20.0 * 30.0).abs() < 1e-6, "{}", volume(&ev));
    assert_eq!(tabs.users(0), [mold]);
    // Cambia el original: la derivada lo sigue (y la huella cambia)
    let before = {
        let mut d = tabs.design(mold).unwrap().clone();
        tabs.fill_derived(mold, &mut d, &mut |d| d.evaluate());
        d.geometry_hash()
    };
    *tabs.design_mut(0).unwrap() = box_doc(25.0);
    let ev = eval_tab(&tabs, mold);
    assert!((volume(&ev) - 15.0 * 20.0 * 30.0).abs() < 1e-6, "{}", volume(&ev));
    let mut d = tabs.design(mold).unwrap().clone();
    tabs.fill_derived(mold, &mut d, &mut |d| d.evaluate());
    assert_ne!(d.geometry_hash(), before);
    // Como pieza aparte: dos piezas
    let mut copy = Document::new();
    copy.add(derived(0, BodyOp::New));
    let c = tabs.add(TabKind::Design { document: Box::new(copy) }, None);
    let ev = eval_tab(&tabs, c);
    assert_eq!(ev.parts.len(), 1);
    assert!((volume(&ev) - 25.0 * 20.0 * 30.0).abs() < 1e-6);
}

#[test]
fn derived_reports_cycles_and_missing_tabs() {
    if !occt::available() {
        return;
    }
    let mut tabs = Tabs::from_legacy(Some(box_doc(10.0)));
    let b = tabs.add(TabKind::Design { document: Box::new(box_doc(40.0)) }, Some("B".into()));
    tabs.design_mut(0).unwrap().add(derived(b, BodyOp::Join));
    tabs.design_mut(b).unwrap().add(derived(0, BodyOp::Join));
    let message = |ev: &Evaluation| ev.errors().first().map(|(_, m)| m.to_string()).unwrap_or_default();
    let ev = eval_tab(&tabs, 0);
    assert!(message(&ev).contains("ciclo"), "{}", message(&ev));
    assert!(message(&ev).contains("«Diseño 1» → «B» → «Diseño 1»"), "{}", message(&ev));
    // Ella misma
    let mut own = tabs.clone();
    own.design_mut(b).unwrap().features.pop();
    own.design_mut(b).unwrap().add(derived(b, BodyOp::Join));
    assert!(message(&eval_tab(&own, b)).contains("ciclo"));
    assert_eq!(own.users(b), [0], "no se cuenta a sí misma");
    // Pestaña borrada o un ensamble
    let asm = tabs.add_assembly(None);
    tabs.design_mut(b).unwrap().features.pop();
    tabs.design_mut(b).unwrap().add(derived(asm, BodyOp::New));
    assert!(message(&eval_tab(&tabs, b)).contains("ensamble"));
    tabs.design_mut(b).unwrap().features.pop();
    tabs.design_mut(b).unwrap().add(derived(77, BodyOp::New));
    assert!(message(&eval_tab(&tabs, b)).contains("ya no está"));
    // Pieza que el original ya no tiene: aviso, no error
    tabs.design_mut(b).unwrap().features.pop();
    let FeatureKind::Derived(mut d) = derived(0, BodyOp::New) else { panic!() };
    let first = tabs.design(0).unwrap().features[0].id;
    d.parts = vec![PartId { feature: first, index: 0 }, PartId { feature: first, index: 9 }];
    tabs.design_mut(0).unwrap().features.pop();
    tabs.design_mut(b).unwrap().add(FeatureKind::Derived(d));
    let ev = eval_tab(&tabs, b);
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 2, "el bloque y la caja traída");
    let st = ev.status.last().unwrap();
    assert!(matches!(&st.state, FeatureState::Warning { missing, .. } if missing.len() == 1 && missing[0].index == 1), "{:?}", st.state);
}

#[test]
fn derived_uses_the_named_configuration() {
    if !occt::available() {
        return;
    }
    let mut a = box_doc(10.0);
    a.parameters.push(Parameter { name: "largo".into(), expr: "10".into() });
    let first = a.features[0].id.0;
    a.bindings.insert(format!("{first}.kind.shape.dx"), "largo".into());
    a.configurations.push(Configuration { name: "Larga".into(), values: [("largo".to_string(), "50".to_string())].into(), suppressed: vec![] });
    let mut tabs = Tabs::from_legacy(Some(a));
    let FeatureKind::Derived(mut d) = derived(0, BodyOp::New) else { panic!() };
    d.configuration = Some("Larga".into());
    let mut doc = Document::new();
    doc.add(FeatureKind::Derived(d.clone()));
    let b = tabs.add(TabKind::Design { document: Box::new(doc) }, None);
    let ev = eval_tab(&tabs, b);
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert!((volume(&ev) - 50.0 * 20.0 * 30.0).abs() < 1e-6, "{}", volume(&ev));
    d.configuration = Some("Otra".into());
    tabs.design_mut(b).unwrap().features[0].kind = FeatureKind::Derived(d);
    assert!(!eval_tab(&tabs, b).errors().is_empty());
}

#[test]
fn instances_in_other_configurations_are_variants() {
    let mut a = box_doc(10.0);
    a.configurations.push(Configuration { name: "Larga".into(), values: Default::default(), suppressed: vec![] });
    a.parts.push(PartProps { part: PartId { feature: a.features[0].id, index: 0 }, name: Some("Caja".into()), color: None, hidden: false, material: None });
    let mut tabs = Tabs::from_legacy(Some(a));
    let b = tabs.add_design(None);
    let asm = tabs.add_assembly(None);
    let part = PartId { feature: FeatureId(0), index: 0 };
    let inst = |id, studio, cfg: Option<&str>| Instance { id, studio, configuration: cfg.map(Into::into), part, name: format!("I{id}"), position: [0.0; 3], rotation: [0.0; 3], fixed: false };
    let assembly = Assembly { instances: vec![inst(1, 0, None), inst(2, 0, Some("Larga")), inst(3, b, Some("X")), inst(4, 0, Some("Larga"))], next_id: 5, ..Default::default() };
    let TabKind::Assembly { assembly: stored, .. } = &mut tabs.get_mut(asm).unwrap().kind else { panic!() };
    *stored = assembly.clone();
    assert_eq!(tabs.variants(asm), [(0, "Larga".to_string()), (b, "X".to_string())]);
    // Cada variante con su número; la misma variante, el mismo
    let flat = tabs.flat_assembly(asm).unwrap();
    let tab_of = |i: usize| split_tab_feature(flat.instances[i].part.feature).0;
    assert_eq!([tab_of(0), tab_of(1), tab_of(2), tab_of(3)], [0, VARIANT_BASE, VARIANT_BASE + 1, VARIANT_BASE]);
    assert_eq!(flat.instances[2].studio, b, "cada instancia conserva su pestaña");
    // Ida y vuelta, también después de cambiarle la configuración a una
    tabs.set_flat_assembly(asm, flat.clone()).unwrap();
    let TabKind::Assembly { assembly: back, .. } = &tabs.get(asm).unwrap().kind else { panic!() };
    assert_eq!(*back, assembly);
    let mut changed = flat;
    changed.instances[0].configuration = Some("Larga".into());
    tabs.set_flat_assembly(asm, changed).unwrap();
    assert_eq!(split_tab_feature(tabs.flat_assembly(asm).unwrap().instances[0].part.feature).0, VARIANT_BASE);
    // Sus documentos: con la configuración activa (la que no existe, la de base)
    let docs = tabs.variant_documents(asm);
    assert_eq!(docs.iter().map(|(v, s, d)| (*v, *s, d.active_configuration)).collect::<Vec<_>>(), [(VARIANT_BASE, 0, Some(0)), (VARIANT_BASE + 1, b, None)]);
    // Los nombres llevan la configuración
    let unnamed = PartId { feature: tab_feature(VARIANT_BASE + 1, FeatureId(0)), index: 0 };
    let (props, _) = tabs.flat_part_props(asm, &[(unnamed, "Pieza 1".into())]);
    let name = |p: PartId| props.iter().find(|x| x.part == p).and_then(|x| x.name.clone());
    assert_eq!(name(PartId { feature: tab_feature(VARIANT_BASE, FeatureId(0)), index: 0 }).as_deref(), Some("Caja (Larga)"));
    assert_eq!(name(PartId { feature: FeatureId(0), index: 0 }).as_deref(), Some("Caja"));
    assert_eq!(name(unnamed).as_deref(), Some("Pieza 1 (X)"));
    // Sin configuraciones, se guarda como antes
    let json = serde_json::to_string(&inst(1, 0, None)).unwrap();
    assert!(!json.contains("configuration"), "{json}");
}
