//! Operaciones desde sketch: casos que daban geometría equivocada o perdían
//! los nombres de las caras (auditoría de 2026-11).

use approx::assert_relative_eq;
use cad_model::*;
use std::f64::consts::PI;

fn occt() -> bool {
    occt::available()
}

fn extrude(sketch: FeatureId, extent: Extent) -> Extrude {
    Extrude { sketch, regions: RegionSelection::All, extent, reverse: false, op: BodyOp::Join, draft: 0.0, thin: None, direction: None, start: 0.0 }
}

fn sketch_at(doc: &mut Document, offset: f64, s: Sketch) -> FeatureId {
    doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xy, offset, sketch: s })
}

fn eval_ok(doc: &Document) -> Evaluation {
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    ev
}

fn volume(ev: &Evaluation) -> f64 {
    ev.body.as_ref().unwrap().mass().unwrap().volume
}

/// El aviso de una operación (vacío si no hay).
fn warning(ev: &Evaluation, id: FeatureId) -> String {
    ev.status
        .iter()
        .find(|s| s.id == id)
        .and_then(|s| match &s.state {
            FeatureState::Warning { message, .. } => Some(message.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn error(ev: &Evaluation, id: FeatureId) -> String {
    ev.errors().into_iter().find(|(f, _)| *f == id).map(|(_, m)| m).unwrap_or_default()
}

/// Caras del cuerpo con ese nombre.
fn named(ev: &Evaluation, name: &str) -> Vec<usize> {
    (0..ev.face_tags.len()).filter(|&f| ev.face_tags[f].iter().any(|t| t.name == name)).collect()
}

fn plate(origin: [f64; 3], dx: f64, dy: f64, dz: f64) -> FeatureKind {
    FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx, dy, dz, centered: true, centered_z: false },
        origin,
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    })
}

#[test]
fn transicion_entre_anillos_conserva_el_hueco() {
    if !occt() {
        return;
    }
    let ring = || {
        let mut s = Sketch::new();
        s.circle([0.0, 0.0], 10.0);
        s.circle([0.0, 0.0], 5.0);
        s
    };
    let mut doc = Document::new();
    let a = sketch_at(&mut doc, 0.0, ring());
    let b = sketch_at(&mut doc, 20.0, ring());
    doc.add(FeatureKind::Loft(Loft {
        sections: vec![LoftSection { sketch: a, regions: RegionSelection::All }, LoftSection { sketch: b, regions: RegionSelection::All }],
        ruled: false,
        op: BodyOp::Join,
        start_point: None,
        end_point: None,
        closed: false,
        thin: None,
        path: None,
        guide: None,
        start_tangent: None,
        end_tangent: None,
    }));
    let ev = eval_ok(&doc);
    assert_relative_eq!(volume(&ev), PI * (100.0 - 25.0) * 20.0, max_relative = 1e-3);
}

#[test]
fn extrusion_hasta_una_cara_inclinada() {
    if !occt() {
        return;
    }
    // Un bloque inclinado (su cara de abajo sube 1 en y cada 2 en z) arriba del sketch
    let tilt = [0.0, 0.5f64, 1.0];
    let n = (tilt[1] * tilt[1] + tilt[2] * tilt[2]).sqrt();
    let z = [0.0, tilt[1] / n, tilt[2] / n];
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 40.0, dy: 40.0, dz: 5.0, centered: true, centered_z: false },
        origin: [0.0, 0.0, 20.0],
        z,
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = eval_ok(&doc);
    let body = ev.body.as_ref().unwrap();
    let bottom = (0..body.face_count()).find(|&f| body.face_info(f).unwrap().normal[2] < -0.8).unwrap();
    let face = ev.face_ref(bottom).unwrap();
    let mut s = Sketch::new();
    s.rectangle([-5.0, -5.0], [5.0, 5.0]);
    let sk = sketch_at(&mut doc, 0.0, s);
    let mut e = extrude(sk, Extent::UpToFace { face, offset: 0.0 });
    e.op = BodyOp::New;
    let ext = doc.add(FeatureKind::Extrude(e));
    let ev = eval_ok(&doc);
    let (tool, _) = ev.tool(ext).unwrap();
    let m = tool.mass().unwrap();
    // Arriba sigue al plano: de 17,5 (y = 5) a 22,5 (y = −5); en el medio, 20
    assert_relative_eq!(m.bbox_max[2], 22.5, epsilon = 1e-6);
    assert_relative_eq!(m.volume, 100.0 * 20.0, max_relative = 1e-6);
    // La tapa de arriba (sobre la cara inclinada) se llama «fin»
    let fin = tool.face_count();
    assert!(fin > 0);
    assert!(!named(&ev, "fin").is_empty(), "la tapa inclinada tiene nombre");
}

#[test]
fn hasta_la_siguiente_desde_un_plano_corrido() {
    if !occt() {
        return;
    }
    // Una placa a z 2..3 (detrás del arranque) y otra a z 12..15
    let mut doc = Document::new();
    doc.add(plate([0.0, 0.0, 2.0], 30.0, 30.0, 1.0));
    doc.add(plate([0.0, 0.0, 12.0], 30.0, 30.0, 3.0));
    let mut s = Sketch::new();
    s.rectangle([-5.0, -5.0], [5.0, 5.0]);
    let sk = sketch_at(&mut doc, 0.0, s);
    let mut e = extrude(sk, Extent::UpToNext);
    e.start = 5.0;
    e.op = BodyOp::New;
    let ext = doc.add(FeatureKind::Extrude(e));
    let ev = eval_ok(&doc);
    let m = ev.tool(ext).unwrap().0.mass().unwrap();
    assert_relative_eq!(m.bbox_min[2], 5.0, epsilon = 1e-6);
    assert_relative_eq!(m.bbox_max[2], 12.0, epsilon = 1e-6);
}

#[test]
fn revolucion_dos_angulos_con_angulo_negativo() {
    if !occt() {
        return;
    }
    let ring = |angle: f64| {
        let mut s = Sketch::new();
        s.rectangle([5.0, 0.0], [8.0, 20.0]);
        let axis = s.line([0.0, 0.0], [0.0, 10.0]);
        s.entities.iter_mut().find(|e| e.id == axis).unwrap().construction = true;
        let mut doc = Document::new();
        let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
        doc.add(FeatureKind::Revolve(Revolve {
            sketch: sk,
            regions: RegionSelection::All,
            axis: AxisSpec::SketchLine { sketch: sk, line: axis },
            angle,
            op: BodyOp::Join,
            extent: RevolveExtent::TwoAngles { second: 30.0 },
            thin: None,
        }));
        eval_ok(&doc)
    };
    let full = PI * (64.0 - 25.0) * 20.0;
    let pos = ring(90.0);
    let neg = ring(-90.0);
    // 90 hacia un lado y 30 hacia el otro: 120 en total, con cualquier signo
    assert_relative_eq!(volume(&neg), full / 3.0, max_relative = 1e-6);
    // Y es el reflejo de la de ángulo positivo
    let (a, b) = (pos.body.as_ref().unwrap().mass().unwrap(), neg.body.as_ref().unwrap().mass().unwrap());
    assert_relative_eq!(a.center[0], b.center[0], epsilon = 1e-6);
    assert_relative_eq!(a.center[1], -b.center[1], epsilon = 1e-6);
}

#[test]
fn revolucion_delgada_que_cruza_el_eje_avisa() {
    if !occt() {
        return;
    }
    let mut s = Sketch::new();
    s.rectangle([0.5, 0.0], [8.0, 20.0]);
    let axis = s.line([0.0, 0.0], [0.0, 10.0]);
    s.entities.iter_mut().find(|e| e.id == axis).unwrap().construction = true;
    let mut doc = Document::new();
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
    let rev = doc.add(FeatureKind::Revolve(Revolve {
        sketch: sk,
        regions: RegionSelection::All,
        axis: AxisSpec::SketchLine { sketch: sk, line: axis },
        angle: 360.0,
        op: BodyOp::Join,
        extent: RevolveExtent::One,
        thin: Some(2.0),
    }));
    let ev = doc.evaluate();
    assert!(error(&ev, rev).contains("cruza el eje"), "{:?}", ev.errors());
}

/// Cada línea del rectángulo nombra al menos una cara; devuelve cuántas.
fn sides_named(ev: &Evaluation, lines: &[u32]) -> Vec<usize> {
    lines.iter().map(|l| named(ev, &format!("lado:{l}")).len()).collect()
}

#[test]
fn laterales_con_nombre_en_delgada_y_con_desmolde() {
    if !occt() {
        return;
    }
    // Delgada: dos paredes por línea (afuera y adentro)
    let mut doc = Document::new();
    let mut s = Sketch::new();
    let lines = s.rectangle([0.0, 0.0], [40.0, 20.0]);
    let sk = sketch_at(&mut doc, 0.0, s);
    let mut e = extrude(sk, Extent::Blind { distance: 10.0 });
    e.thin = Some(2.0);
    doc.add(FeatureKind::Extrude(e));
    assert_eq!(sides_named(&eval_ok(&doc), &lines), vec![2; 4]);

    // Con desmolde, ciega y simétrica
    for extent in [Extent::Blind { distance: 10.0 }, Extent::Symmetric { distance: 10.0 }] {
        let mut doc = Document::new();
        let mut s = Sketch::new();
        let lines = s.rectangle([0.0, 0.0], [40.0, 20.0]);
        let sk = sketch_at(&mut doc, 0.0, s);
        let mut e = extrude(sk, extent);
        e.draft = 5.0;
        doc.add(FeatureKind::Extrude(e));
        let ev = eval_ok(&doc);
        assert!(sides_named(&ev, &lines).iter().all(|&n| n >= 1), "{:?}", sides_named(&ev, &lines));
    }
}

#[test]
fn inicio_queda_en_el_plano_del_sketch_con_distancia_negativa() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [10.0, 10.0]);
    let sk = sketch_at(&mut doc, 0.0, s);
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::Blind { distance: -10.0 })));
    let ev = eval_ok(&doc);
    let body = ev.body.as_ref().unwrap();
    let z = |name: &str| body.face_info(named(&ev, name)[0]).unwrap().point[2];
    assert_relative_eq!(z("inicio"), 0.0, epsilon = 1e-9);
    assert_relative_eq!(z("fin"), -10.0, epsilon = 1e-9);
}

#[test]
fn linea_partida_nombra_todas_sus_caras() {
    if !occt() {
        return;
    }
    // Rectángulo partido al medio: el lado de abajo queda en dos tramos
    let mut doc = Document::new();
    let mut s = Sketch::new();
    let lines = s.rectangle([0.0, 0.0], [40.0, 20.0]);
    s.line([20.0, 0.0], [20.0, 20.0]);
    let sk = sketch_at(&mut doc, 0.0, s);
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::Blind { distance: 10.0 })));
    let ev = eval_ok(&doc);
    let body = ev.body.as_ref().unwrap();
    // Todas las caras sobre y = 0 son del lado de abajo
    let on_bottom: Vec<usize> = (0..body.face_count())
        .filter(|&f| {
            let i = body.face_info(f).unwrap();
            i.normal[1] < -0.999 && i.point[1].abs() < 1e-9
        })
        .collect();
    let bottom = lines.iter().find(|&&l| named(&ev, &format!("lado:{l}")).iter().any(|f| on_bottom.contains(f))).unwrap();
    let mut tagged = named(&ev, &format!("lado:{bottom}"));
    tagged.sort();
    assert_eq!(tagged, on_bottom);
}

#[test]
fn agujero_avisos_y_medidas() {
    if !occt() {
        return;
    }
    let centers = |pts: &[[f64; 2]]| {
        let mut s = Sketch::new();
        for p in pts {
            let id = s.add_point(p[0], p[1]);
            s.add_entity(Geometry::Point { point: id });
        }
        s
    };
    let hole = |sketch, depth, style| Hole {
        sketch,
        points: vec![],
        diameter: 4.0,
        depth,
        style,
        tip_angle: 0.0,
        thread: None,
        modeled: None,
        link: None,
        reverse: None,
        compensate: false,
    };
    // Un centro fuera de la placa: avisa
    let mut doc = Document::new();
    doc.add(plate([0.0, 0.0, -10.0], 20.0, 20.0, 10.0));
    let sk = sketch_at(&mut doc, 0.0, centers(&[[0.0, 0.0], [50.0, 0.0]]));
    let h = doc.add(FeatureKind::Hole(hole(sk, HoleDepth::ThroughAll, HoleStyle::Simple)));
    let ev = eval_ok(&doc);
    assert!(warning(&ev, h).contains("1 de 2 agujeros"), "{}", warning(&ev, h));

    // Caja más honda que el agujero ciego: error
    let mut doc = Document::new();
    doc.add(plate([0.0, 0.0, -10.0], 20.0, 20.0, 10.0));
    let sk = sketch_at(&mut doc, 0.0, centers(&[[0.0, 0.0]]));
    let h = doc.add(FeatureKind::Hole(hole(sk, HoleDepth::Blind { depth: 3.0 }, HoleStyle::Counterbore { diameter: 8.0, depth: 5.0 })));
    assert!(error(&doc.evaluate(), h).contains("más honda"));
}

#[test]
fn nervio_avisa_de_las_curvas() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(plate([0.0, 0.0, 0.0], 40.0, 40.0, 5.0));
    let mut s = Sketch::new();
    s.line([-10.0, 10.0], [10.0, 10.0]);
    s.arc([0.0, 10.0], [5.0, 10.0], [-5.0, 10.0]);
    let sk = doc.add(FeatureKind::Sketch { plane: PlaneSpec::Xz, offset: 0.0, sketch: s });
    let rib = doc.add(FeatureKind::Rib { sketch: sk, thickness: 2.0, flip: false, scan: None });
    let ev = eval_ok(&doc);
    assert!(warning(&ev, rib).contains("curva"), "{}", warning(&ev, rib));
}

// --- Casos dudosos de la auditoría -------------------------------------

#[test]
fn contorno_interior_que_toca_el_borde_en_un_punto() {
    if !occt() {
        return;
    }
    // Triángulo adentro de un cuadrado con la punta sobre el lado de abajo
    let mut doc = Document::new();
    let mut s = Sketch::new();
    s.rectangle([0.0, 0.0], [20.0, 20.0]);
    s.line([10.0, 0.0], [15.0, 10.0]);
    s.line([15.0, 10.0], [5.0, 10.0]);
    s.line([5.0, 10.0], [10.0, 0.0]);
    let sk = sketch_at(&mut doc, 0.0, s);
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::Blind { distance: 1.0 })));
    let ev = eval_ok(&doc);
    // El cuadrado menos el triángulo
    assert_relative_eq!(volume(&ev), 400.0 - 50.0, max_relative = 1e-6);
    assert!(ev.body.as_ref().unwrap().is_valid());

    // Con un clic adentro del triángulo: solo el triángulo
    if let FeatureKind::Extrude(e) = &mut doc.features.last_mut().unwrap().kind {
        e.regions = RegionSelection::Points { points: vec![[10.0, 5.0]] };
    }
    assert_relative_eq!(volume(&eval_ok(&doc)), 50.0, max_relative = 1e-6);
}

#[test]
fn extremos_casi_pegados() {
    if !occt() {
        return;
    }
    // Un cuadrado lejos del origen con una esquina abierta 5 µm
    let mut doc = Document::new();
    let mut s = Sketch::new();
    let p = [[100.0, 100.0], [110.0, 100.0], [110.0, 110.0], [100.0, 110.0]].map(|q: [f64; 2]| s.add_point(q[0], q[1]));
    let gap = s.add_point(100.0 + 5e-6, 100.0);
    s.add_line(p[0], p[1]);
    s.add_line(p[1], p[2]);
    s.add_line(p[2], p[3]);
    s.add_line(p[3], gap);
    let sk = sketch_at(&mut doc, 0.0, s);
    let ext = doc.add(FeatureKind::Extrude(extrude(sk, Extent::Blind { distance: 1.0 })));
    let ev = doc.evaluate();
    // Si el sketch lo da por cerrado, la extrusión tiene que salir
    if !ev.sketches[&sk].regions.is_empty() {
        assert!(error(&ev, ext).is_empty(), "{}", error(&ev, ext));
        assert_relative_eq!(volume(&ev), 100.0, max_relative = 1e-4);
    }
}

#[test]
fn circulos_repetidos_no_duplican() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    let mut s = Sketch::new();
    s.circle([0.0, 0.0], 5.0);
    s.circle([0.0, 0.0], 5.0);
    let sk = sketch_at(&mut doc, 0.0, s);
    doc.add(FeatureKind::Extrude(extrude(sk, Extent::Blind { distance: 2.0 })));
    let ev = eval_ok(&doc);
    assert_relative_eq!(volume(&ev), PI * 25.0 * 2.0, max_relative = 1e-6);
}
