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
    b.link = Some(GearLink { feature: ia, angle, offset: 0.0, distance: None });
    doc.add(prim(PrimitiveShape::Gear(b)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    assert_eq!(ev.parts.len(), 3);
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, part: ev.parts[k].id, name: format!("P{id}"), position: [0.0; 3], rotation: [0.0; 3], fixed };
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
    let (ev, mut asm) = train(gear(GearKind::External, 18), 0.0);
    let sol = solve(&asm);
    let rel = suggest_gear_relations(&ev, &asm, &sol);
    assert_eq!(rel.len(), 1);
    let Relation::Gear { a, b, ratio, phase } = rel[0].clone() else { panic!() };
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
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let ia = doc.add(prim(PrimitiveShape::Gear(gear(GearKind::External, 12))));
    let mut ring = gear(GearKind::Internal, 30);
    ring.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None });
    doc.add(prim(PrimitiveShape::Gear(ring)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
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
    let mut doc = Document::new();
    doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
    let ia = doc.add(prim(PrimitiveShape::Gear(gear(GearKind::External, 12))));
    let mut rack = GearPart { spec: GearSpec { kind: GearKind::Rack, teeth: 8, module: 2.0, ..Default::default() }, width: 10.0, outer: 6.0, ..Default::default() };
    rack.link = Some(GearLink { feature: ia, angle: -90.0, offset: 0.0, distance: None });
    doc.add(prim(PrimitiveShape::Gear(rack)));
    let ev = doc.evaluate();
    assert!(ev.errors().is_empty(), "{:?}", ev.errors());
    let inst = |id: u32, k: usize, fixed: bool| Instance { id, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
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
        let a = Instance { id: 1, part: PartId { feature: FeatureId(0), index: 0 }, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed: true };
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
    assert_eq!(r, Relation::Gear { a: 1, b: 2, ratio: -0.5, phase: 0.0 });
    let old: Assembly = serde_json::from_value(serde_json::json!({ "instances": [], "mates": [], "next_id": 1 })).unwrap();
    assert!(old.relations.is_empty());
    assert!(serde_json::to_value(&old).unwrap().get("relations").is_none());
}

#[test]
fn sin_fin_y_rueda_en_el_ensamble() {
    for left in [false, true] {
        let mut doc = Document::new();
        doc.add(prim(PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 2.0, centered: false, centered_z: false }));
        let worm = GearPart { spec: GearSpec { kind: GearKind::Worm, teeth: 1, module: 2.0, left_hand: left, ..Default::default() }, width: 45.0, ..Default::default() };
        let ia = doc.add(prim(PrimitiveShape::Gear(worm)));
        let mut wheel = gear(GearKind::External, 20);
        wheel.width = 12.0;
        wheel.link = Some(GearLink { feature: ia, angle: 0.0, offset: 0.0, distance: None });
        doc.add(prim(PrimitiveShape::Gear(wheel)));
        let ev = doc.evaluate();
        assert!(ev.errors().is_empty(), "{:?}", ev.errors());
        let inst = |id: u32, k: usize, fixed: bool| Instance { id, part: ev.parts[k].id, name: String::new(), position: [0.0; 3], rotation: [0.0; 3], fixed };
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
