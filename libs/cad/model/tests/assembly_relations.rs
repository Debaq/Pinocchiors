//! Relaciones del ensamble: engranes, piñón-cremallera y tornillo.

use approx::assert_relative_eq;
use cad_model::assembly::{instance_shapes, interferences_for, measure, solve, suggest_gear_relations};
use cad_model::gear_part::{GearLink, GearPart};
use cad_model::gears::{GearKind, GearSpec};
use cad_model::*;

fn prim(shape: PrimitiveShape) -> FeatureKind {
    FeatureKind::Primitive(Primitive { shape, origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0], op: BodyOp::New, link: None })
}

fn gear(kind: GearKind, z: u32) -> GearPart {
    GearPart { spec: GearSpec { kind, teeth: z, module: 2.0, ..Default::default() }, width: 10.0, outer: if kind == GearKind::Internal { 80.0 } else { 6.0 }, ..Default::default() }
}

fn conn(instance: u32, origin: P3, z: P3, x: P3) -> Connector {
    Connector { instance, origin, z, x }
}

fn hinge(id: u32, kind: MateKind, housing: Connector, part: Connector) -> Mate {
    Mate { id, name: String::new(), kind, a: housing, b: part, flip: false, angle: None, distance: None }
}

/// Carcasa (caja fija) + dos engranes engranados, cada uno con su bisagra a la carcasa
fn train(second: GearPart, angle: f64) -> (Evaluation, Assembly) {
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let ia = doc.add(prim(PrimitiveShape::Gear(gear(GearKind::External, 12))));
    let mut b = second;
    b.link = Some(GearLink { feature: ia, angle, offset: 0.0, distance: None, along: 0.0 });
    doc.add(prim(PrimitiveShape::Gear(b)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 3);
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, studio: 0, part: ev.parts[k].id, name: format!("P{id}"), position: [0.0; 3], rotation: [0.0; 3], fixed };
    let (ga, gb) = (&ev.gears[0], &ev.gears[1]);
    let asm = Assembly {
        instances: vec![inst(1, 0, true), inst(2, 1, false), inst(3, 2, false)],
        mates: vec![
            hinge(1, MateKind::Revolute, conn(1, ga.origin, ga.z, ga.x), conn(2, ga.origin, ga.z, ga.x)),
            hinge(2, MateKind::Revolute, conn(1, gb.origin, gb.z, gb.x), conn(3, gb.origin, gb.z, gb.x)),
        ],
        next_id: 4,
        ..Default::default()
    };
    (ev, asm)
}

fn z_turn(sol: &cad_model::AssemblySolution, id: u32) -> f64 {
    sol.poses.iter().find(|p| p.0 == id).unwrap().2[2].to_degrees()
}

#[test]
fn engranes_sugeridos_con_su_razon_real() {
    if !occt::available() {
        return;
    }
    let (ev, mut asm) = train(gear(GearKind::External, 18), 0.0);
    let sol = solve(&asm);
    let rel = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(rel.len(), 1);
    let Relation::Gear { a, b, ratio, phase, .. } = rel[0].clone() else { panic!() };
    assert_eq!((a, b), (1, 2));
    assert_relative_eq!(ratio, -12.0 / 18.0, epsilon = 1e-12);
    assert_relative_eq!(phase, 0.0, epsilon = 1e-6);
    // Animar el primero gira el segundo al revés y en proporción
    asm.relations = rel;
    asm.mates[0].angle = Some(30.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(z_turn(&sol, 2), 30.0, epsilon = 1e-6);
    assert_relative_eq!(z_turn(&sol, 3), -20.0, epsilon = 1e-6);
    // Y al revés: imponer el segundo mueve el primero
    asm.mates[0].angle = None;
    asm.mates[1].angle = Some(-40.0);
    let sol = solve(&asm);
    assert_relative_eq!(z_turn(&sol, 2), 60.0, epsilon = 1e-6);
}

#[test]
fn relacion_sin_unir_la_resuelve_el_solver() {
    if !occt::available() {
        return;
    }
    let (ev, mut asm) = train(gear(GearKind::External, 18), 0.0);
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    // Sin ángulos impuestos, un grado libre en vez de dos
    let sol = solve(&asm);
    assert_eq!(sol.dof, 1);
    assert!(sol.converged);
    // Mueve el primero a mano (sin fijar su ángulo): el solver arrastra el otro
    asm.instances[1].rotation = [0.0, 0.0, 0.5];
    let sol = solve(&asm);
    assert!(sol.converged, "residuo {}", sol.residual);
    let (ta, _) = measure(&asm, &sol, 1).unwrap();
    let (tb, _) = measure(&asm, &sol, 2).unwrap();
    assert_relative_eq!(tb, -2.0 / 3.0 * ta, epsilon = 1e-4);
}

#[test]
fn los_dientes_giran_sin_chocar() {
    if !occt::available() {
        return;
    }
    let (ev, mut asm) = train(gear(GearKind::External, 18), 40.0);
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    for angle in [0.0, 17.0, 95.0] {
        asm.mates[0].angle = Some(angle);
        let sol = solve(&asm);
        assert!(sol.converged);
        let shapes: Vec<_> = instance_shapes(&ev, &asm, &sol).into_iter().filter(|(id, _)| *id != 1).collect();
        let hits = interferences_for(&ev, &asm, &shapes);
        assert!(hits.is_empty(), "a {angle}°: {hits:?}");
        // Control: con la fase corrida medio paso sí chocan
        if angle == 17.0 {
            let mut bad = asm.clone();
            if let Relation::Gear { phase, .. } = &mut bad.relations[0] {
                *phase += 10.0;
            }
            let sol = solve(&bad);
            let shapes: Vec<_> = instance_shapes(&ev, &bad, &sol).into_iter().filter(|(id, _)| *id != 1).collect();
            assert!(!cad_model::assembly::interferences(&shapes).is_empty(), "el control debe chocar");
        }
    }
}

#[test]
fn corona_gira_con_el_piñon() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let ia = doc.add(prim(PrimitiveShape::Gear(gear(GearKind::External, 12))));
    let mut ring = gear(GearKind::Internal, 30);
    ring.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None, along: 0.0 });
    doc.add(prim(PrimitiveShape::Gear(ring)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, studio: 0, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
    let (ga, gb) = (&ev.gears[0], &ev.gears[1]);
    let mut asm = Assembly {
        instances: vec![inst(1, 0, true), inst(2, 1, false), inst(3, 2, false)],
        mates: vec![
            hinge(1, MateKind::Revolute, conn(1, ga.origin, ga.z, ga.x), conn(2, ga.origin, ga.z, ga.x)),
            hinge(2, MateKind::Revolute, conn(1, gb.origin, gb.z, gb.x), conn(3, gb.origin, gb.z, gb.x)),
        ],
        next_id: 4,
        ..Default::default()
    };
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    let Relation::Gear { ratio, .. } = asm.relations[0] else { panic!() };
    assert_relative_eq!(ratio, 12.0 / 30.0, epsilon = 1e-12);
}

#[test]
fn piñon_y_cremallera() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let ia = doc.add(prim(PrimitiveShape::Gear(gear(GearKind::External, 12))));
    let mut rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 8, module: 2.0, ..Default::default() }, width: 10.0, outer: 6.0, ..Default::default() };
    rack.link = Some(GearLink { feature: ia, angle: -90.0, offset: 0.0, distance: None, along: 0.0 });
    doc.add(prim(PrimitiveShape::Gear(rack)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, studio: 0, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
    let (ga, gr) = (&ev.gears[0], &ev.gears[1]);
    // La cremallera desliza a lo largo de su X; su conector lleva Z = ese eje
    let slide_z = gr.x;
    let slide_x = gr.z;
    let mut asm = Assembly {
        instances: vec![inst(1, 0, true), inst(2, 1, false), inst(3, 2, false)],
        mates: vec![
            hinge(1, MateKind::Revolute, conn(1, ga.origin, ga.z, ga.x), conn(2, ga.origin, ga.z, ga.x)),
            hinge(2, MateKind::Slider, conn(1, gr.origin, slide_z, slide_x), conn(3, gr.origin, slide_z, slide_x)),
        ],
        next_id: 4,
        ..Default::default()
    };
    let sol = solve(&asm);
    let rel = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(rel.len(), 1);
    let Relation::RackPinion { per_turn, .. } = rel[0] else { panic!() };
    assert_relative_eq!(per_turn.abs(), std::f64::consts::TAU * 12.0, epsilon = 1e-9);
    asm.relations = rel;
    asm.mates[0].angle = Some(36.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    let (_, d) = measure(&asm, &sol, 2).unwrap();
    assert_relative_eq!(d.abs(), 0.1 * per_turn.abs(), epsilon = 1e-6);
    if occt::available() {
        let shapes: Vec<_> = instance_shapes(&ev, &asm, &sol).into_iter().filter(|(id, _)| *id != 1).collect();
        let hits = cad_model::assembly::interferences(&shapes);
        let tooth = shapes[0].1.mass().unwrap().volume / 12.0;
        assert!(hits.iter().all(|h| h.2 < 5e-3 * tooth), "choca: {hits:?}");
    }
}

#[test]
fn tornillo_acopla_giro_y_avance() {
    let mk = |angle: Option<f64>, distance: Option<f64>| {
        let a = Instance { id: 1, studio: 0, part: PartId { feature: FeatureId(0), index: 0 }, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed: true };
        let b = Instance { id: 2, ..a.clone() };
        let b = Instance { fixed: false, ..b };
        let c = |i| conn(i, [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let mut m = hinge(1, MateKind::Cylindrical, c(1), c(2));
        m.angle = angle;
        m.distance = distance;
        Assembly { instances: vec![a, b], mates: vec![m], relations: vec![Relation::Screw { cylindrical: 1, pitch: 1.5, phase: 0.0 }], next_id: 3 }
    };
    let sol = solve(&mk(Some(720.0), None));
    let (_, d) = measure(&mk(Some(720.0), None), &sol, 1).unwrap();
    assert_relative_eq!(d, 3.0, epsilon = 1e-9);
    let asm = mk(None, Some(0.375));
    let sol = solve(&asm);
    assert_relative_eq!(z_turn(&sol, 2), 90.0, epsilon = 1e-6);
}

#[test]
fn json_de_las_relaciones() {
    let r: Relation = serde_json::from_value(serde_json::json!({ "type": "gear", "a": 1, "b": 2, "ratio": -0.5 })).unwrap();
    assert_eq!(r, Relation::Gear { a: 1, b: 2, ratio: -0.5, phase: 0.0, carrier: None, carrier_ratio: 0.0 });
    let old: Assembly = serde_json::from_value(serde_json::json!({ "instances": [], "mates": [], "next_id": 1 })).unwrap();
    assert!(old.relations.is_empty());
    assert!(serde_json::to_value(&old).unwrap().get("relations").is_none());
}

#[test]
fn sin_fin_y_rueda_en_el_ensamble() {
    if !occt::available() {
        return;
    }
    for left in [false, true] {
        let mut doc = Document::new();
        doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
        let worm = GearPart { spec: GearSpec { kind: GearKind::Worm, teeth: 1, module: 2.0, left_hand: left, ..Default::default() }, width: 45.0, ..Default::default() };
        let ia = doc.add(prim(PrimitiveShape::Gear(worm)));
        let mut wheel = gear(GearKind::External, 20);
        wheel.width = 12.0;
        wheel.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None, along: 0.0 });
        doc.add(prim(PrimitiveShape::Gear(wheel)));
        let ev = doc.evaluate();
        assert!(ev.errors().is_empty(), "{:?}", ev.errors());
        let inst = |id: u32, k: usize, fixed: bool| Instance { id, studio: 0, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
        let (ga, gb) = (&ev.gears[0], &ev.gears[1]);
        let mut asm = Assembly {
            instances: vec![inst(1, 0, true), inst(2, 1, false), inst(3, 2, false)],
            mates: vec![
                hinge(1, MateKind::Revolute, conn(1, ga.origin, ga.z, ga.x), conn(2, ga.origin, ga.z, ga.x)),
                hinge(2, MateKind::Revolute, conn(1, gb.origin, gb.z, gb.x), conn(3, gb.origin, gb.z, gb.x)),
            ],
            next_id: 4,
            ..Default::default()
        };
        let sol = solve(&asm);
        asm.relations = suggest_gear_relations(&ev, &asm, &sol);
        let Relation::Gear { ratio, .. } = asm.relations[0] else { panic!() };
        // 1 entrada, 20 dientes: la rueda gira 1/20 y el signo sigue la mano y los ejes
        assert_relative_eq!(ratio.abs(), 1.0 / 20.0, epsilon = 1e-12);
        asm.mates[0].angle = Some(90.0);
        let sol = solve(&asm);
        assert!(sol.converged);
        if occt::available() {
            let shapes: Vec<_> = instance_shapes(&ev, &asm, &sol).into_iter().filter(|(id, _)| *id != 1).collect();
            let tooth = shapes[1].1.mass().unwrap().volume / 20.0;
            let hits = cad_model::assembly::interferences(&shapes);
            assert!(hits.iter().all(|h| h.2 < 0.03 * tooth), "choca girando 90°: {hits:?}");
            // Control: con el signo de la razón al revés la rueda queda medio paso corrida
            let mut bad = asm.clone();
            if let Relation::Gear { ratio, phase, .. } = &mut bad.relations[0] {
                *ratio = -*ratio;
                *phase = 0.0;
            }
            let sol = solve(&bad);
            let shapes: Vec<_> = instance_shapes(&ev, &bad, &sol).into_iter().filter(|(id, _)| *id != 1).collect();
            let hits = cad_model::assembly::interferences(&shapes);
            assert!(hits.iter().any(|h| h.2 > 0.1 * tooth), "el control debe chocar: {hits:?}");
        }
    }
}

#[test]
fn sale_del_punto_muerto_de_180_grados() {
    if !occt::available() {
        return;
    }
    // Dientes iguales: razón -1; los dos a 90° dejan la relación desfasada justo 180°
    let (ev, mut asm) = train(gear(GearKind::External, 12), 0.0);
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    asm.instances[1].rotation = [0.0, 0.0, std::f64::consts::FRAC_PI_2];
    asm.instances[2].rotation = [0.0, 0.0, std::f64::consts::FRAC_PI_2];
    let sol = solve(&asm);
    assert!(sol.converged, "residuo {}", sol.residual);
    let (ta, _) = measure(&asm, &sol, 1).unwrap();
    let (tb, _) = measure(&asm, &sol, 2).unwrap();
    assert_relative_eq!(tb, -ta, epsilon = 1e-4);
}

/// Carcasa fija + engranes encadenados (cada uno enlazado al anterior), todos con bisagra a la carcasa.
/// `parts`: (engrane, ángulo del centro respecto del anterior).
fn chain(parts: Vec<(GearPart, f64)>, extra_hinge_first: bool) -> (Evaluation, Assembly) {
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let mut prev = None;
    for (mut g, angle) in parts {
        if let Some(feature) = prev {
            g.link = Some(GearLink { feature, angle, offset: 0.0, distance: None, along: 0.0 });
        }
        prev = Some(doc.add(prim(PrimitiveShape::Gear(g))));
    }
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let n = ev.gears.len();
    let mut instances = vec![Instance { id: 1, studio: 0, part: ev.parts[0].id, name: "C".into(), position: [0.0; 3], rotation: [0.0; 3], fixed: true }];
    let mut mates = Vec::new();
    for (k, g) in ev.gears.iter().enumerate() {
        let id = k as u32 + 2;
        instances.push(Instance { id, studio: 0, part: ev.parts[k + 1].id, name: format!("G{id}"), position: [0.0; 3], rotation: [0.0; 3], fixed: false });
        if extra_hinge_first && k == 1 {
            // Una bisagra de eje perpendicular (otra pieza) antes de la buena: no debe confundir la detección
            mates.push(hinge(100, MateKind::Revolute, conn(1, g.origin, g.x, g.z), conn(id, g.origin, g.x, g.z)));
        }
        mates.push(hinge(k as u32 + 1, MateKind::Revolute, conn(1, g.origin, g.z, g.x), conn(id, g.origin, g.z, g.x)));
    }
    let _ = n;
    (ev, Assembly { instances, mates, next_id: 200, ..Default::default() })
}

#[test]
fn tren_de_tres_engranes_con_loco() {
    if !occt::available() {
        return;
    }
    let (ev, mut asm) = chain(vec![(gear(GearKind::External, 12), 0.0), (gear(GearKind::External, 18), 0.0), (gear(GearKind::External, 24), 90.0)], false);
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(asm.relations.len(), 2);
    asm.mates[0].angle = Some(36.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    // 12→18: -2/3; 18→24: -3/4; total +1/2
    assert_relative_eq!(z_turn(&sol, 3), -24.0, epsilon = 1e-6);
    assert_relative_eq!(z_turn(&sol, 4), 18.0, epsilon = 1e-6);
    // Imponer el último gira todo el tren hacia atrás
    asm.mates[0].angle = None;
    asm.mates[2].angle = Some(-10.0);
    let sol = solve(&asm);
    assert_relative_eq!(z_turn(&sol, 2), -20.0, epsilon = 1e-6);
}

#[test]
fn la_deteccion_elige_la_bisagra_del_eje_del_engrane() {
    if !occt::available() {
        return;
    }
    let (ev, mut asm) = chain(vec![(gear(GearKind::External, 12), 0.0), (gear(GearKind::External, 18), 0.0)], true);
    let sol = solve(&asm);
    let rel = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(rel.len(), 1);
    let Relation::Gear { a, b, .. } = rel[0] else { panic!() };
    assert_eq!((a, b), (1, 2), "{rel:?}");
    asm.relations = rel;
}

#[test]
fn tren_con_corona_de_cuatro_engranes() {
    if !occt::available() {
        return;
    }
    // Sol 12 → satélite 12 → corona 36 (carro fijo): la corona gira 12/36 del sol y en el mismo sentido
    let (ev, mut asm) = chain(vec![(gear(GearKind::External, 12), 0.0), (gear(GearKind::External, 12), 0.0), (gear(GearKind::Internal, 36), 0.0)], false);
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(asm.relations.len(), 2);
    asm.mates[0].angle = Some(90.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(z_turn(&sol, 3), -90.0, epsilon = 1e-6);
    assert_relative_eq!(z_turn(&sol, 4), -30.0, epsilon = 1e-6);
}

#[test]
fn planetario_con_brazo_que_gira() {
    if !occt::available() {
        return;
    }
    // Sol 12 → satélite 12 (montado en el brazo) → corona 36. Willis: ω_c − c = −(z_s/z_c)·(ω_s − c)
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let sun = doc.add(prim(PrimitiveShape::Gear(gear(GearKind::External, 12))));
    let mut planet = gear(GearKind::External, 12);
    planet.link = Some(GearLink { feature: sun, angle: 0.0, offset: 0.0, distance: None, along: 0.0 });
    let planet = doc.add(prim(PrimitiveShape::Gear(planet)));
    let mut ring = gear(GearKind::Internal, 36);
    ring.link = Some(GearLink { feature: planet, angle: 0.0, offset: 0.0, distance: None, along: 0.0 });
    doc.add(prim(PrimitiveShape::Gear(ring)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, studio: 0, part: ev.parts[k].id, name: format!("P{id}"), position: [0.0; 3], rotation: [0.0; 3], fixed };
    let (gs, gp, gr) = (&ev.gears[0], &ev.gears[1], &ev.gears[2]);
    let mut asm = Assembly {
        instances: vec![inst(1, 0, true), inst(2, 1, false), inst(3, 0, false), inst(4, 2, false), inst(5, 3, false)],
        mates: vec![
            hinge(1, MateKind::Revolute, conn(1, gs.origin, gs.z, gs.x), conn(2, gs.origin, gs.z, gs.x)),
            hinge(2, MateKind::Revolute, conn(1, gs.origin, gs.z, gs.x), conn(3, gs.origin, gs.z, gs.x)),
            hinge(3, MateKind::Revolute, conn(3, gp.origin, gp.z, gp.x), conn(4, gp.origin, gp.z, gp.x)),
            hinge(4, MateKind::Revolute, conn(1, gr.origin, gr.z, gr.x), conn(5, gr.origin, gr.z, gr.x)),
        ],
        next_id: 10,
        ..Default::default()
    };
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(asm.relations.len(), 2, "{:?}", asm.relations);
    assert!(asm.relations.iter().any(|r| matches!(r, Relation::Gear { carrier: Some(2), .. })));
    // Sol a 60° y brazo a 30°: satélite (relativo al brazo) −30°, corona 20°
    asm.mates[0].angle = Some(60.0);
    asm.mates[1].angle = Some(30.0);
    let sol = solve(&asm);
    assert!(sol.converged, "residuo {}", sol.residual);
    assert_relative_eq!(measure(&asm, &sol, 3).unwrap().0, -30.0, epsilon = 1e-6);
    assert_relative_eq!(measure(&asm, &sol, 4).unwrap().0, 20.0, epsilon = 1e-6);
    // Brazo parado: como el tren fijo, la corona gira −1/3 del sol
    asm.mates[1].angle = Some(0.0);
    let sol = solve(&asm);
    assert_relative_eq!(measure(&asm, &sol, 4).unwrap().0, -20.0, epsilon = 1e-6);
    // Al revés: imponer la corona y el brazo mueve el sol
    asm.mates[0].angle = None;
    asm.mates[1].angle = Some(30.0);
    asm.mates[3].angle = Some(20.0);
    let sol = solve(&asm);
    assert_relative_eq!(measure(&asm, &sol, 1).unwrap().0, 60.0, epsilon = 1e-6);
    // Sin imponer nada, el solver los acopla: quedan 2 grados libres (4 uniones − 2 relaciones)
    asm.mates.iter_mut().for_each(|m| m.angle = None);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_eq!(sol.dof, 2);
}

#[test]
fn avisa_de_ejes_no_paralelos_y_de_helicoidales_cruzados() {
    if !occt::available() {
        return;
    }
    use cad_model::assembly::gear_axis_warnings;
    let (ev, mut asm) = chain(vec![(gear(GearKind::External, 12), 0.0), (gear(GearKind::External, 18), 0.0)], false);
    let sol = solve(&asm);
    assert!(gear_axis_warnings(&ev, &asm, &sol).is_empty());
    // Se tumba el segundo engrane 90°: ejes perpendiculares
    asm.instances[2].rotation = [std::f64::consts::FRAC_PI_2, 0.0, 0.0];
    asm.instances[2].fixed = true;
    let sol = solve(&asm);
    let w = gear_axis_warnings(&ev, &asm, &sol);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("90.0°") && !w[0].contains("cruzados"), "{}", w[0]);
    // Con los dos helicoidales avisa también de los cruzados
    let helical = |z, beta| GearPart { spec: GearSpec { teeth: z, module: 2.0, helix_angle: beta, ..Default::default() }, width: 10.0, outer: 6.0, ..Default::default() };
    let (ev, mut asm) = chain(vec![(helical(12, 15.0), 0.0), (helical(18, -15.0), 0.0)], false);
    asm.instances[2].rotation = [std::f64::consts::FRAC_PI_2, 0.0, 0.0];
    asm.instances[2].fixed = true;
    let sol = solve(&asm);
    let w = gear_axis_warnings(&ev, &asm, &sol);
    assert!(w.len() == 1 && w[0].contains("cruzados"), "{w:?}");
}

#[test]
fn cremallera_primero_y_piñon_despues_en_el_ensamble() {
    if !occt::available() {
        return;
    }
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 8, module: 2.0, ..Default::default() }, width: 10.0, outer: 6.0, ..Default::default() };
    let ir = doc.add(prim(PrimitiveShape::Gear(rack)));
    let mut pinion = gear(GearKind::External, 12);
    pinion.link = Some(GearLink { feature: ir, angle: 0.0, offset: 0.0, distance: None, along: 5.0 });
    doc.add(prim(PrimitiveShape::Gear(pinion)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, studio: 0, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
    let (gr, gp) = (&ev.gears[0], &ev.gears[1]);
    let mut asm = Assembly {
        instances: vec![inst(1, 0, true), inst(2, 1, false), inst(3, 2, false)],
        mates: vec![
            hinge(1, MateKind::Slider, conn(1, gr.origin, gr.x, gr.z), conn(2, gr.origin, gr.x, gr.z)),
            hinge(2, MateKind::Revolute, conn(1, gp.origin, gp.z, gp.x), conn(3, gp.origin, gp.z, gp.x)),
        ],
        next_id: 4,
        ..Default::default()
    };
    let sol = solve(&asm);
    let rel = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(rel.len(), 1, "{rel:?}");
    let Relation::RackPinion { revolute, slider, per_turn, .. } = rel[0] else { panic!() };
    assert_eq!((revolute, slider), (2, 1));
    assert_relative_eq!(per_turn.abs(), std::f64::consts::TAU * 12.0, epsilon = 1e-9);
    asm.relations = rel;
    asm.mates[1].angle = Some(36.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(measure(&asm, &sol, 1).unwrap().1.abs(), 0.1 * per_turn.abs(), epsilon = 1e-6);
    if occt::available() {
        let hits_for = |asm: &Assembly| {
            let sol = solve(asm);
            let shapes: Vec<_> = instance_shapes(&ev, asm, &sol).into_iter().filter(|(id, _)| *id != 1).collect();
            (shapes[1].1.mass().unwrap().volume / 12.0, cad_model::assembly::interferences(&shapes))
        };
        let (tooth, hits) = hits_for(&asm);
        assert!(hits.iter().all(|h| h.2 < 5e-3 * tooth), "choca: {hits:?}");
        // Control: con el sentido de la relación al revés, el avance sale para el otro lado y choca
        let mut bad = asm.clone();
        if let Relation::RackPinion { per_turn, phase, .. } = &mut bad.relations[0] {
            *per_turn = -*per_turn;
            *phase = 0.0;
        }
        let (tooth, hits) = hits_for(&bad);
        assert!(hits.iter().any(|h| h.2 > 0.05 * tooth), "el control debe chocar: {hits:?}");
    }
}

#[test]
fn rueda_primero_y_sin_fin_despues_en_el_ensamble() {
    if !occt::available() {
        return;
    }
    for left in [false, true] {
        // La rueda como la deja el sin fin, pero va primero
        let mut forward = Document::new();
        let worm = GearPart { spec: GearSpec { kind: GearKind::Worm, teeth: 1, module: 2.0, left_hand: left, ..Default::default() }, width: 45.0, ..Default::default() };
        let iw = forward.add(prim(PrimitiveShape::Gear(worm.clone())));
        let mut wheel = gear(GearKind::External, 20);
        wheel.width = 12.0;
        wheel.link = Some(GearLink { feature: iw, angle: 0.0, offset: 0.0, distance: None, along: 0.0 });
        forward.add(prim(PrimitiveShape::Gear(wheel)));
        let mut first = forward.evaluate().gears[1].part.clone();
        first.link = None;
        let mut doc = Document::new();
        doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
        let ia = doc.add(prim(PrimitiveShape::Gear(first)));
        let mut w = worm;
        w.link = Some(GearLink { feature: ia, angle: 30.0, offset: 0.0, distance: None, along: 0.0 });
        doc.add(prim(PrimitiveShape::Gear(w)));
        let ev = doc.evaluate();
        assert!(ev.errors().is_empty(), "{:?}", ev.errors());
        let inst = |id: u32, k: usize, fixed: bool| Instance { id, studio: 0, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
        let (ga, gb) = (&ev.gears[0], &ev.gears[1]);
        let mut asm = Assembly {
            instances: vec![inst(1, 0, true), inst(2, 1, false), inst(3, 2, false)],
            mates: vec![
                hinge(1, MateKind::Revolute, conn(1, ga.origin, ga.z, ga.x), conn(2, ga.origin, ga.z, ga.x)),
                hinge(2, MateKind::Revolute, conn(1, gb.origin, gb.z, gb.x), conn(3, gb.origin, gb.z, gb.x)),
            ],
            next_id: 4,
            ..Default::default()
        };
        let sol = solve(&asm);
        asm.relations = suggest_gear_relations(&ev, &asm, &sol);
        let Relation::Gear { ratio, .. } = asm.relations[0] else { panic!() };
        // El sin fin gira 20 vueltas por vuelta de la rueda
        assert_relative_eq!(ratio.abs(), 20.0, epsilon = 1e-9);
        asm.mates[0].angle = Some(4.5);
        let sol = solve(&asm);
        assert!(sol.converged);
        if occt::available() {
            let hits_for = |asm: &Assembly| {
                let sol = solve(asm);
                let shapes: Vec<_> = instance_shapes(&ev, asm, &sol).into_iter().filter(|(id, _)| *id != 1).collect();
                (shapes[0].1.mass().unwrap().volume / 20.0, cad_model::assembly::interferences(&shapes))
            };
            let (tooth, hits) = hits_for(&asm);
            assert!(hits.iter().all(|h| h.2 < 0.03 * tooth), "choca girando 4,5°: {hits:?}");
            let mut bad = asm.clone();
            if let Relation::Gear { ratio, phase, .. } = &mut bad.relations[0] {
                *ratio = -*ratio;
                *phase = 0.0;
            }
            let (tooth, hits) = hits_for(&bad);
            assert!(hits.iter().any(|h| h.2 > 0.1 * tooth), "el control debe chocar: {hits:?}");
        }
    }
}

#[test]
fn un_angulo_impuesto_de_180_grados_converge() {
    if !occt::available() {
        return;
    }
    let (_, mut asm) = train(gear(GearKind::External, 18), 0.0);
    asm.mates[0].angle = Some(180.0);
    let sol = solve(&asm);
    assert!(sol.converged, "residuo {}", sol.residual);
    assert_relative_eq!(measure(&asm, &sol, 1).unwrap().0.abs(), 180.0, epsilon = 1e-4);
    // Y un conjunto imposible sigue sin converger (no se inventa una solución)
    asm.mates[1].angle = Some(10.0);
    asm.mates.push(Mate { id: 9, name: String::new(), kind: MateKind::Fastened, a: asm.mates[1].a, b: asm.mates[1].b, flip: false, angle: None, distance: None });
    asm.mates.push(Mate { id: 10, name: String::new(), kind: MateKind::Fastened, a: asm.mates[1].a, b: Connector { origin: [5.0, 0.0, 0.0], ..asm.mates[1].b }, flip: false, angle: None, distance: None });
    assert!(!solve(&asm).converged);
}

/// Planetario completo: sol 24, tres satélites de 12 y corona de 48 (fija), con un portasatélites
fn planetary(n: u32, hub_angle_check: bool) -> (Evaluation, Assembly) {
    let mut doc = Document::new();
    let sun = doc.add(prim(PrimitiveShape::Gear(gear(GearKind::External, 24))));
    let mut planets = Vec::new();
    for k in 0..n {
        let mut p = gear(GearKind::External, 12);
        p.link = Some(GearLink { feature: sun, angle: 360.0 * k as f64 / n as f64, offset: 0.0, distance: None, along: 0.0 });
        planets.push(doc.add(prim(PrimitiveShape::Gear(p))));
    }
    // La dirección del centro se mide desde la X del satélite (que gira para calzar): para que la corona
    // quede concentrica con el sol, hacia -X del mundo, hay que restar lo que giró esa X
    let first = doc.evaluate();
    let x = first.gears[1].x;
    let psi = x[1].atan2(x[0]).to_degrees();
    let mut ring = gear(GearKind::Internal, 48);
    ring.outer = 110.0;
    ring.link = Some(GearLink { feature: planets[0], angle: 180.0 - psi, offset: 0.0, distance: None, along: 0.0 });
    doc.add(prim(PrimitiveShape::Gear(ring)));
    doc.add(prim(PrimitiveShape::Cylinder { radius: 40.0, height: 3.0 }));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let g = &ev.gears;
    if hub_angle_check {
        // La corona queda concentrica con el sol
        for k in 0..3 {
            assert_relative_eq!(g[n as usize + 1].origin[k], g[0].origin[k], epsilon = 1e-9);
        }
    }
    // Instancias: 1 corona (fija), 2 sol, 3 portasatélites, luego los satélites
    let part = |k: usize| ev.parts[k].id;
    let ring_part = n as usize + 2; // la corona es la parte siguiente a sol y satélites
    let mut instances = vec![
        Instance { id: 1, studio: 0, part: part(ring_part - 1), name: "Corona".into(), position: [0.0; 3], rotation: [0.0; 3], fixed: true },
        Instance { id: 2, studio: 0, part: part(0), name: "Sol".into(), position: [0.0; 3], rotation: [0.0; 3], fixed: false },
        Instance { id: 3, studio: 0, part: part(ring_part), name: "Portasatélites".into(), position: [0.0; 3], rotation: [0.0; 3], fixed: false },
    ];
    let sun_axis = &g[0];
    let mut mates = vec![
        hinge(1, MateKind::Revolute, conn(1, sun_axis.origin, sun_axis.z, sun_axis.x), conn(2, sun_axis.origin, sun_axis.z, sun_axis.x)),
        hinge(2, MateKind::Revolute, conn(1, sun_axis.origin, sun_axis.z, sun_axis.x), conn(3, sun_axis.origin, sun_axis.z, sun_axis.x)),
    ];
    for k in 0..n as usize {
        let pa = &g[1 + k];
        let id = 4 + k as u32;
        instances.push(Instance { id, studio: 0, part: part(1 + k), name: format!("Satélite {}", k + 1), position: [0.0; 3], rotation: [0.0; 3], fixed: false });
        mates.push(hinge(3 + k as u32, MateKind::Revolute, conn(3, pa.origin, pa.z, pa.x), conn(id, pa.origin, pa.z, pa.x)));
    }
    (ev, Assembly { instances, mates, next_id: 100, ..Default::default() })
}

#[test]
fn planetario_completo_con_la_corona_fija() {
    if !occt::available() {
        return;
    }
    // Las partes: 0 sol, 1..3 satélites, 4 corona, 5 portasatélites
    let (ev, mut asm) = planetary(3, true);
    assert_eq!(ev.parts.len(), 6);
    // Con el orden de las partes (sol, satélites, corona, portasatélites) la corona es la 5.ª
    asm.instances[0].part = ev.parts[4].id;
    asm.instances[2].part = ev.parts[5].id;
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    // Sol → cada satélite (con el brazo) y, con la corona quieta, brazo → satélite 1 (la corona engrana con el 1.º;
    // los demás quedan atados por el sol, sin ecuaciones de más)
    assert_eq!(asm.relations.len(), 4, "{:?}", asm.relations);
    assert_eq!(asm.relations.iter().filter(|r| matches!(r, Relation::Gear { carrier: Some(2), .. })).count(), 3);
    // El sol a 72°: el portasatélites va a 72·24/(24+48) = 24° y cada satélite gira −(48/12)·24 = −96° respecto del brazo
    asm.mates[0].angle = Some(72.0);
    let sol = solve(&asm);
    assert!(sol.converged, "residuo {}", sol.residual);
    assert_relative_eq!(measure(&asm, &sol, 2).unwrap().0, 24.0, epsilon = 1e-4);
    for id in 3..6 {
        assert_relative_eq!(measure(&asm, &sol, id).unwrap().0, -96.0, epsilon = 1e-4);
    }
    // Un grado libre: sol y brazo van atados por la corona quieta
    asm.mates[0].angle = None;
    assert_eq!(solve(&asm).dof, 1);
}

#[test]
fn planetario_gira_sin_chocar() {
    if !occt::available() {
        return;
    }
    let (ev, mut asm) = planetary(3, false);
    asm.instances[0].part = ev.parts[4].id;
    asm.instances[2].part = ev.parts[5].id;
    let sol = solve(&asm);
    asm.relations = suggest_gear_relations(&ev, &asm, &sol);
    for angle in [0.0, 30.0] {
        asm.mates[0].angle = Some(angle);
        let sol = solve(&asm);
        assert!(sol.converged);
        let shapes: Vec<_> = instance_shapes(&ev, &asm, &sol).into_iter().filter(|(id, _)| *id != 3).collect();
        let tooth = shapes[0].1.mass().unwrap().volume / 24.0;
        let hits = interferences_for(&ev, &asm, &shapes);
        assert!(hits.iter().all(|h| h.2 < 0.01 * tooth), "a {angle}°: {hits:?}");
    }
}
