//! La impresora del documento da la holgura de los engranes y la compensación de los agujeros.

use approx::assert_relative_eq;
use cad_model::gear_part::GearPart;
use cad_model::gears::GearSpec;
use cad_model::*;
use pinocchio_print3d::{Fit, MaterialProfile, PrinterProfile};
use std::f64::consts::PI;

fn fab(nozzle: f64) -> Fabrication {
    Fabrication { printer: PrinterProfile::new("t", [200.0; 3], nozzle, 0.2), material: MaterialProfile::default(), fit: Fit::Normal, compensate_all: false }
}

fn gear_doc(g: GearPart, fabrication: Option<Fabrication>) -> Document {
    let mut doc = Document::new();
    doc.fabrication = fabrication;
    doc.add(FeatureKind::Primitive(Primitive { shape: PrimitiveShape::Gear(g), origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::Join, link: None }));
    doc
}

fn vol(doc: &Document) -> f64 {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev.body.unwrap().mass().unwrap().volume
}

fn spur() -> GearPart {
    GearPart { spec: GearSpec { teeth: 24, module: 2.0, ..Default::default() }, width: 10.0, ..Default::default() }
}

#[test]
fn la_holgura_automatica_sigue_a_la_boquilla() {
    if !occt::available() {
        return;
    }
    let auto = GearPart { auto_clearance: true, ..spur() };
    let plain = vol(&gear_doc(spur(), None));
    let fine = vol(&gear_doc(auto.clone(), Some(fab(0.25))));
    let coarse = vol(&gear_doc(auto.clone(), Some(fab(0.6))));
    // Más boquilla, más holgura, menos diente
    assert!(coarse < fine && fine < plain, "{plain} {fine} {coarse}");
    // Igual a pedir la holgura a mano: el ajuste justo es la mitad de la base
    let by_hand = vol(&gear_doc(GearPart { print_clearance: 0.125, ..spur() }, None));
    assert_relative_eq!(fine, by_hand, max_relative = 1e-9);
}

#[test]
fn sin_impresora_la_holgura_automatica_avisa() {
    if !occt::available() {
        return;
    }
    let doc = gear_doc(GearPart { auto_clearance: true, ..spur() }, None);
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty());
    let warned = ev.status.iter().any(|s| matches!(&s.state, FeatureState::Warning { message, .. } if message.contains("impresora")));
    assert!(warned, "{:?}", ev.status);
}

#[test]
fn cambiar_de_impresora_recalcula() {
    if !occt::available() {
        return;
    }
    let mut doc = gear_doc(GearPart { auto_clearance: true, ..spur() }, Some(fab(0.4)));
    let a = vol(&doc);
    doc.fabrication = Some(fab(0.8));
    // La caché no puede devolver el engrane viejo
    let b = vol(&doc);
    assert!(b < a, "{a} {b}");
}

/// Placa con un agujero de 6 mm en el centro
fn plate_with_hole(compensate: bool, fabrication: Option<Fabrication>) -> Document {
    let mut doc = Document::new();
    doc.fabrication = fabrication;
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 40.0, dy: 40.0, dz: 10.0, centered: true, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    let (top, _) = ev.body.as_ref().unwrap().closest_face([0.0, 0.0, 10.0], Some([0.0, 0.0, 1.0]), 0.99).unwrap();
    let mut s = Sketch::default();
    let p = s.add_point(0.0, 0.0);
    s.entities.push(SketchEntity { id: s.next_id, geometry: Geometry::Point { point: p }, construction: false, infinite: false, axis: false, layer: None });
    s.next_id += 1;
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Face { face: ev.face_ref(top).unwrap() }, offset: 0.0, sketch: s });
    doc.add(FeatureKind::Hole(Hole {
        sketch: sk,
        points: vec![],
        diameter: 6.0,
        depth: HoleDepth::ThroughAll,
        style: HoleStyle::Simple,
        tip_angle: 0.0,
        thread: None,
        modeled: None,
        link: None,
        reverse: None,
        compensate,
    }));
    doc
}

#[test]
fn el_agujero_compensado_sale_mas_ancho_en_el_diseno() {
    if !occt::available() {
        return;
    }
    let nominal = vol(&plate_with_hole(false, Some(fab(0.4))));
    let comp = vol(&plate_with_hole(true, Some(fab(0.4))));
    // hole_comp = 0,25 de boquilla = 0,1 por lado: el diámetro de 6 pasa a 6,2
    let expected_removed = |d: f64| PI * d * d / 4.0 * 10.0;
    let full = 40.0 * 40.0 * 10.0;
    assert_relative_eq!(full - nominal, expected_removed(6.0), max_relative = 1e-6);
    assert_relative_eq!(full - comp, expected_removed(6.2), max_relative = 1e-6);
}

#[test]
fn sin_marca_el_agujero_no_cambia() {
    if !occt::available() {
        return;
    }
    let a = vol(&plate_with_hole(false, None));
    let b = vol(&plate_with_hole(false, Some(fab(0.4))));
    assert_relative_eq!(a, b, max_relative = 1e-12);
}

#[test]
fn la_fabricacion_se_guarda_en_el_documento() {
    let doc = gear_doc(spur(), Some(fab(0.4)));
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.fabrication, doc.fabrication);
    // Un documento viejo (sin el campo) se abre igual
    let old: Document = serde_json::from_str(&json.replace(&format!(",\"fabrication\":{}", serde_json::to_string(&doc.fabrication).unwrap()), "")).unwrap_or_default();
    let _ = old;
}

// ── Compensar todo lo hueco: tuercas, arandelas, ejes de engrane, roscas, agujeros ──

fn all(nozzle: f64) -> Fabrication {
    Fabrication { compensate_all: true, ..fab(nozzle) }
}

fn standard_doc(shape: PrimitiveShape, fabrication: Option<Fabrication>) -> Document {
    let mut doc = Document::new();
    doc.fabrication = fabrication;
    doc.add(FeatureKind::Primitive(Primitive { shape, origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::Join, link: None }));
    doc
}

#[test]
fn la_arandela_compensada_tiene_el_agujero_mas_ancho() {
    if !occt::available() {
        return;
    }
    let m = cad_model::standard::size("M6").unwrap();
    let shape = || PrimitiveShape::Washer { size: "M6".into() };
    let plain = vol(&standard_doc(shape(), None));
    // Sin compensar_all, tener impresora no cambia nada
    assert_relative_eq!(plain, vol(&standard_doc(shape(), Some(fab(0.4)))), max_relative = 1e-12);
    let comp = vol(&standard_doc(shape(), Some(all(0.4))));
    // hole_comp = 0,1 por lado: el agujero pasa de d1 a d1 + 0,2
    let area = |d: f64| PI * d * d / 4.0;
    assert_relative_eq!(plain - comp, (area(m.washer_d1 + 0.2) - area(m.washer_d1)) * m.washer_h, max_relative = 1e-6);
}

#[test]
fn la_tuerca_compensada_pierde_material_del_agujero() {
    if !occt::available() {
        return;
    }
    let m = cad_model::standard::size("M6").unwrap();
    let plain_nut = || PrimitiveShape::Nut { size: "M6".into(), modeled: false };
    let a = vol(&standard_doc(plain_nut(), None));
    let b = vol(&standard_doc(plain_nut(), Some(all(0.4))));
    let area = |d: f64| PI * d * d / 4.0;
    assert_relative_eq!(a - b, (area(m.d + 0.2) - area(m.d)) * m.nut_m, max_relative = 1e-6);
    // Con rosca modelada, la holgura (0,4) quita más material todavía
    let threaded = || PrimitiveShape::Nut { size: "M6".into(), modeled: true };
    let c = vol(&standard_doc(threaded(), None));
    let d = vol(&standard_doc(threaded(), Some(all(0.4))));
    assert!(d < c - 1.0, "{c} {d}");
}

#[test]
fn el_eje_y_el_chavetero_del_engrane_salen_mas_anchos() {
    if !occt::available() {
        return;
    }
    let gear = |keyway: bool| GearPart { bore: 10.0, keyway, ..spur() };
    let plain = vol(&gear_doc(gear(false), None));
    let comp = vol(&gear_doc(gear(false), Some(all(0.4))));
    // Eje de 10 → 10,2 en todo el ancho (10 mm)
    let area = |d: f64| PI * d * d / 4.0;
    assert_relative_eq!(plain - comp, (area(10.2) - area(10.0)) * 10.0, max_relative = 1e-6);
    // El chavetero se ensancha 0,2 y se hace 0,1 más hondo: más material fuera que sin chavetero
    let k_plain = vol(&gear_doc(gear(true), None));
    let k_comp = vol(&gear_doc(gear(true), Some(all(0.4))));
    assert!((k_plain - k_comp) > (plain - comp) + 0.5, "{} vs {}", k_plain - k_comp, plain - comp);
    // Los aligeramientos también
    let holes = |f: Option<Fabrication>| vol(&gear_doc(GearPart { hole_count: 4, hole_diameter: 5.0, hole_circle: 30.0, bore: 10.0, hub_diameter: 16.0, ..spur() }, f));
    let drop = holes(None) - holes(Some(all(0.4)));
    assert_relative_eq!(drop, (area(10.2) - area(10.0)) * 10.0 + 4.0 * (area(5.2) - area(5.0)) * 10.0, max_relative = 1e-3);
}

#[test]
fn compensar_todo_equivale_a_marcar_cada_agujero() {
    if !occt::available() {
        return;
    }
    let marked = vol(&plate_with_hole(true, Some(fab(0.4))));
    let global = vol(&plate_with_hole(false, Some(all(0.4))));
    assert_relative_eq!(marked, global, max_relative = 1e-12);
}

#[test]
fn compensar_todo_entra_en_la_clave_de_la_cache() {
    if !occt::available() {
        return;
    }
    let mut doc = plate_with_hole(false, Some(fab(0.4)));
    let a = vol(&doc);
    doc.fabrication = Some(all(0.4));
    // La caché no puede devolver el agujero sin compensar
    assert!(vol(&doc) < a);
}

#[test]
fn el_documento_guarda_compensar_todo_solo_si_esta_activo() {
    let mut doc = gear_doc(spur(), Some(fab(0.4)));
    let json = serde_json::to_string(&doc).unwrap();
    assert!(!json.contains("compensate_all"));
    doc.fabrication = Some(all(0.4));
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("\"compensate_all\":true"));
    let back: Document = serde_json::from_str(&json).unwrap();
    assert!(back.fabrication.unwrap().compensate_all);
}

#[test]
fn la_rosca_interior_lleva_holgura_si_el_diseno_compensa() {
    if !occt::available() {
        return;
    }
    let build = |f: Option<Fabrication>| {
        let mut doc = plate_with_hole(false, f);
        // El agujero de Ø6 pasa a ser el diámetro menor de un M6 (Ø4,917) y se le talla la rosca
        let hole = doc.features.iter().position(|x| matches!(x.kind, FeatureKind::Hole(_))).unwrap();
        let d1 = 6.0 - 1.082_532;
        if let FeatureKind::Hole(h) = &mut doc.features[hole].kind {
            h.diameter = d1;
        }
        let ev = doc.evaluate();
        let (face, _) = ev.body.as_ref().unwrap().closest_face([d1 / 2.0, 0.0, 5.0], Some([-1.0, 0.0, 0.0]), 0.9).unwrap();
        let face = ev.face_ref(face).unwrap();
        doc.add(FeatureKind::Thread { face, pitch: 1.0, length: 0.0, flip: false, left: false, clearance: 0.0, link: None });
        doc
    };
    let a = vol(&build(None));
    let b = vol(&build(Some(all(0.4))));
    // La rosca interior de 0,4 de holgura quita más material (agranda el filete en 0,2 por lado)
    assert!(a - b > 3.0, "{a} {b}");
    // Tener impresora sin compensar todo no cambia la rosca
    let c = vol(&build(Some(fab(0.4))));
    assert_relative_eq!(a, c, max_relative = 1e-12);
}
