//! PLAN_ENSAMBLE: limpieza (EN0), conectores ligados a la pieza (EN1), uniones
//! nuevas y topes (EN4), diagnóstico y rendimiento (EN5), patrones, grupos y
//! espejo (EN8), animación por pasos (EN10) y explosión (EN11).

use approx::assert_relative_eq;
use cad_model::assembly::{explode_offsets, instance_matrix, solve, sweep, sweep_steps, Pose};
use cad_model::assembly_refs::{connector_at, part_face_ref, prepared};
use cad_model::*;

fn conn(instance: u32, origin: P3, z: P3) -> Connector {
    Connector { instance, origin, z, x: if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }, ..Default::default() }
}

fn inst(id: u32, position: P3, fixed: bool) -> Instance {
    Instance { id, name: format!("I{id}"), position, fixed, ..Default::default() }
}

fn mate(id: u32, kind: MateKind, a: Connector, b: Connector) -> Mate {
    Mate { id, kind, a, b, ..Default::default() }
}

fn apply(m: [[f64; 4]; 3], p: P3) -> P3 {
    [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3])
}

fn pose(sol: &AssemblySolution, id: u32) -> [[f64; 4]; 3] {
    let (_, p, r) = sol.poses.iter().find(|x| x.0 == id).unwrap();
    instance_matrix(*p, *r)
}

/// Caja fija y una segunda pieza apoyada encima (a la que se le aplica `kind`).
fn pair(kind: MateKind) -> Assembly {
    Assembly {
        instances: vec![inst(1, [0.0; 3], true), inst(2, [20.0, 3.0, 7.0], false)],
        mates: vec![Mate { flip: true, ..mate(1, kind, conn(1, [0.0, 0.0, 10.0], [0.0, 0.0, 1.0]), conn(2, [0.0; 3], [0.0, 0.0, -1.0])) }],
        next_id: 3,
        ..Default::default()
    }
}

#[test]
fn en0_borrar_union_limpia_acoplamientos() {
    let mut asm = pair(MateKind::Revolute);
    asm.instances.push(inst(3, [0.0; 3], false));
    asm.mates.push(mate(2, MateKind::Revolute, conn(1, [30.0, 0.0, 0.0], [0.0, 0.0, 1.0]), conn(3, [0.0; 3], [0.0, 0.0, 1.0])));
    asm.relations.push(Relation::Gear { a: 1, b: 2, ratio: -1.0, phase: 0.0, carrier: None, carrier_ratio: 0.0 });
    asm.motion.push(MotionStep { mate: 2, var: SweepVar::Angle, from: 0.0, to: 90.0, seconds: 1.0 });
    asm.mates.retain(|m| m.id != 2);
    asm.prune();
    assert!(asm.relations.is_empty(), "acoplamiento a una unión borrada");
    assert!(asm.motion.is_empty());
    // Borrar una instancia se lleva sus uniones (y con ellas, sus acoplamientos)
    let mut asm = pair(MateKind::Revolute);
    asm.groups.push(InstanceGroup { id: 9, name: String::new(), members: vec![1, 2] });
    asm.instances.retain(|i| i.id != 2);
    asm.prune();
    assert!(asm.mates.is_empty());
    assert!(asm.groups.is_empty(), "un grupo de una sola pieza no tiene sentido");
}

fn box_doc(dx: f64) -> Document {
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx, dy: 10.0, dz: 10.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::New,
        link: None,
        on: None,
    }));
    doc
}

#[test]
fn en1_la_union_sigue_a_su_cara() {
    if !occt::available() {
        return;
    }
    let ev = box_doc(10.0).evaluate();
    let part = &ev.parts[0];
    // La cara +X de la caja
    let face = (0..part.shape.face_count()).find(|&f| part.shape.face_info(f).unwrap().normal[0] > 0.9).unwrap();
    let at = ConnectorRef::Face { face: part_face_ref(part, face).unwrap() };
    let a = connector_at(part, 1, at.clone()).unwrap();
    assert_relative_eq!(a.origin[0], 10.0, epsilon = 1e-9);
    let b = connector_at(part, 2, ConnectorRef::Face { face: part_face_ref(part, (0..part.shape.face_count()).find(|&f| part.shape.face_info(f).unwrap().normal[0] < -0.9).unwrap()).unwrap() }).unwrap();
    let mut asm = Assembly {
        instances: vec![Instance { part: part.id, ..inst(1, [0.0; 3], true) }, Instance { part: part.id, ..inst(2, [40.0, 0.0, 0.0], false) }],
        mates: vec![Mate { flip: true, ..mate(1, MateKind::Fastened, a, b) }],
        next_id: 3,
        ..Default::default()
    };
    let (ready, lost) = prepared(&ev, &asm);
    assert!(lost.is_empty());
    let sol = solve(&ready);
    assert!(sol.converged);
    assert_relative_eq!(pose(&sol, 2)[0][3], 10.0, epsilon = 1e-6);
    // La caja crece a 25 mm: la segunda pieza sigue pegada a la cara +X, ahora en x = 25
    let ev2 = box_doc(25.0).evaluate();
    let (ready, lost) = prepared(&ev2, &asm);
    assert!(lost.is_empty(), "{lost:?}");
    assert_relative_eq!(ready.mates[0].a.origin[0], 25.0, epsilon = 1e-9);
    let sol = solve(&ready);
    assert!(sol.converged);
    assert_relative_eq!(pose(&sol, 2)[0][3], 25.0, epsilon = 1e-6);
    // Corrimiento y giro del conector
    asm.mates[0].a.offset = 2.0;
    let (ready, _) = prepared(&ev2, &asm);
    assert_relative_eq!(ready.mates[0].a.origin[0], 27.0, epsilon = 1e-9);
    // La cara ya no está (la pieza es otra cosa): aviso y queda lo guardado
    let mut cyl = Document::new();
    cyl.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Cylinder { radius: 3.0, height: 4.0 },
        origin: [0.0, 0.0, -300.0],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::New,
        link: None,
        on: None,
    }));
    let ev3 = cyl.evaluate();
    let (ready, lost) = prepared(&ev3, &asm);
    assert!(!lost.is_empty(), "la cara se perdió");
    assert_eq!(ready.mates[0].a.origin, asm.mates[0].a.origin);
}

#[test]
fn en4_separacion_angulo_tangente_y_centrada() {
    // Plana con separación: B queda 5 mm arriba
    let mut asm = pair(MateKind::Planar);
    asm.mates[0].distance = Some(5.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(apply(pose(&sol, 2), [0.0; 3])[2], 15.0, epsilon = 1e-6);
    // Pegada con separación
    let mut asm = pair(MateKind::Fastened);
    asm.mates[0].distance = Some(3.0);
    let sol = solve(&asm);
    assert!(sol.converged && sol.dof == 0);
    assert_relative_eq!(apply(pose(&sol, 2), [0.0; 3])[2], 13.0, epsilon = 1e-6);
    // Ángulo de 30° entre los Z
    let mut asm = pair(MateKind::Angle);
    asm.mates[0].flip = false;
    asm.mates[0].b.z = [0.0, 0.0, 1.0];
    asm.mates[0].angle = Some(30.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    let m = pose(&sol, 2);
    assert_relative_eq!(m[2][2], 30f64.to_radians().cos(), epsilon = 1e-6);
    // Tangente: cilindro de radio 4 (eje X de B) apoyado sobre el plano de A
    let mut asm = pair(MateKind::Tangent);
    asm.mates[0].flip = false;
    asm.mates[0].b = Connector { radius: 4.0, ..conn(2, [0.0; 3], [1.0, 0.0, 0.0]) };
    let sol = solve(&asm);
    assert!(sol.converged, "{sol:?}");
    assert_relative_eq!(apply(pose(&sol, 2), [0.0; 3])[2], 14.0, epsilon = 1e-6);
    // Cilindro contra cilindro: ejes a 3 + 4 mm
    let mut asm = pair(MateKind::Tangent);
    asm.mates[0].a = Connector { radius: 3.0, ..conn(1, [0.0; 3], [0.0, 0.0, 1.0]) };
    asm.mates[0].b = Connector { radius: 4.0, ..conn(2, [0.0; 3], [0.0, 0.0, 1.0]) };
    asm.mates[0].flip = false;
    let sol = solve(&asm);
    assert!(sol.converged);
    let o = apply(pose(&sol, 2), [0.0; 3]);
    assert_relative_eq!(o[0].hypot(o[1]), 7.0, epsilon = 1e-6);
    // Centrada entre las caras z = 0 y z = 10 de A
    let mut asm = pair(MateKind::Width);
    asm.mates[0].a = conn(1, [0.0; 3], [0.0, 0.0, 1.0]);
    asm.mates[0].c = Some(conn(1, [0.0, 0.0, 10.0], [0.0, 0.0, -1.0]));
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(apply(pose(&sol, 2), [0.0; 3])[2], 5.0, epsilon = 1e-6);
}

#[test]
fn en4_topes_detienen_bisagra_y_deslizante() {
    // Ángulo pedido más allá del tope: se queda en el tope
    let mut asm = pair(MateKind::Revolute);
    asm.mates[0].angle_limits = Some([-30.0, 45.0]);
    asm.mates[0].angle = Some(120.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    let m = pose(&sol, 2);
    // X de B: girado 45° (con flip, Z de B al revés: medimos con el ángulo de la unión)
    let (t, _) = assembly::measure(&asm, &sol, 1).unwrap();
    assert_relative_eq!(t, 45.0, epsilon = 1e-4);
    let _ = m;
    // Recorrido: el deslizante se detiene en el tope
    let mut asm = pair(MateKind::Slider);
    asm.mates[0].distance_limits = Some([0.0, 8.0]);
    let frames = sweep(&asm, 1, SweepVar::Distance, 0.0, 20.0, 5).unwrap();
    let last = frames.last().unwrap();
    assert_relative_eq!(apply(pose(last, 2), [0.0; 3])[2], 18.0, epsilon = 1e-6);
    // Libre, fuera del tope: el solver lo trae al tope
    let mut asm = pair(MateKind::Slider);
    asm.instances[1].position = [0.0, 0.0, 40.0];
    asm.mates[0].distance_limits = Some([0.0, 8.0]);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert!(apply(pose(&sol, 2), [0.0; 3])[2] <= 18.0 + 1e-6);
}

#[test]
fn en4_acoplamientos_lineal_correa_y_leva() {
    // Dos deslizantes acoplados: B baja la mitad de lo que sube A
    let mut asm = Assembly {
        instances: vec![inst(1, [0.0; 3], true), inst(2, [0.0; 3], false), inst(3, [50.0, 0.0, 0.0], false)],
        mates: vec![
            mate(1, MateKind::Slider, conn(1, [0.0; 3], [0.0, 0.0, 1.0]), conn(2, [0.0; 3], [0.0, 0.0, 1.0])),
            mate(2, MateKind::Slider, conn(1, [50.0, 0.0, 0.0], [0.0, 0.0, 1.0]), conn(3, [0.0; 3], [0.0, 0.0, 1.0])),
        ],
        relations: vec![Relation::Linear { a: 1, b: 2, ratio: -0.5, phase: 0.0 }],
        next_id: 4,
        ..Default::default()
    };
    asm.mates[0].distance = Some(10.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(pose(&sol, 3)[2][3], -5.0, epsilon = 1e-6);
    // Correa: la polea B gira lo mismo que A por 2
    let mut asm = Assembly {
        instances: vec![inst(1, [0.0; 3], true), inst(2, [0.0; 3], false), inst(3, [50.0, 0.0, 0.0], false)],
        mates: vec![
            mate(1, MateKind::Revolute, conn(1, [0.0; 3], [0.0, 0.0, 1.0]), conn(2, [0.0; 3], [0.0, 0.0, 1.0])),
            mate(2, MateKind::Revolute, conn(1, [50.0, 0.0, 0.0], [0.0, 0.0, 1.0]), conn(3, [0.0; 3], [0.0, 0.0, 1.0])),
        ],
        relations: vec![Relation::Belt { a: 1, b: 2, ratio: 2.0, phase: 0.0 }],
        next_id: 4,
        ..Default::default()
    };
    asm.mates[0].angle = Some(30.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(assembly::measure(&asm, &sol, 2).unwrap().0, 60.0, epsilon = 1e-4);
    // Leva: a 90° el seguidor sube 4 mm
    asm.relations = vec![Relation::Cam { revolute: 1, slider: 2, lift: vec![[0.0, 0.0], [180.0, 8.0]], phase: 0.0 }];
    asm.mates[1].kind = MateKind::Slider;
    asm.mates[0].angle = Some(90.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_relative_eq!(pose(&sol, 3)[2][3], 4.0, epsilon = 1e-6);
    assert_relative_eq!(assembly::cam_lift(&[[0.0, 0.0], [180.0, 8.0]], 270.0), 4.0, epsilon = 1e-9);
}

#[test]
fn en5_que_union_falla_cual_sobra_y_quien_se_mueve() {
    // Dos pegadas que se contradicen: las dos salen con desvío
    let mut asm = pair(MateKind::Fastened);
    let mut other = asm.mates[0].clone();
    other.id = 2;
    other.a.origin = [0.0, 0.0, 30.0];
    asm.mates.push(other);
    let sol = solve(&asm);
    assert!(!sol.converged);
    assert_eq!(sol.mate_errors.len(), 2, "{:?}", sol.mate_errors);
    // Una paralela encima de una pegada no agrega nada
    let mut asm = pair(MateKind::Fastened);
    let mut par = asm.mates[0].clone();
    par.id = 2;
    par.kind = MateKind::Parallel;
    asm.mates.push(par);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert!(sol.redundant.contains(&2), "{:?}", sol.redundant);
    // Grados libres por pieza: la bisagra deja a B con 1; una pieza suelta tiene 6
    let mut asm = pair(MateKind::Revolute);
    asm.instances.push(inst(3, [80.0, 0.0, 0.0], false));
    let sol = solve(&asm);
    assert_eq!(sol.instance_dof, vec![(2, 1), (3, 6)]);
    assert_eq!(sol.dof, 7);
}

#[test]
fn en5_cien_piezas_rapido() {
    // 25 módulos de 4 piezas sobre una base fija: 100 piezas y 150 uniones
    let mut asm = Assembly { instances: vec![inst(1, [0.0; 3], true)], next_id: 2, ..Default::default() };
    let mut id = 2;
    let z = [0.0, 0.0, 1.0];
    for k in 0..25 {
        let x0 = k as f64 * 40.0;
        let ids: Vec<u32> = (0..4).map(|n| id + n).collect();
        id += 4;
        for (n, &i) in ids.iter().enumerate() {
            asm.instances.push(inst(i, [x0 + 3.0, 2.0 + n as f64, 5.0 * n as f64 + 1.0], false));
        }
        let m = asm.mates.len() as u32 + 1;
        asm.mates.push(mate(m, MateKind::Revolute, conn(1, [x0, 0.0, 0.0], z), conn(ids[0], [0.0; 3], z)));
        asm.mates.push(mate(m + 1, MateKind::Fastened, conn(ids[0], [0.0, 0.0, 5.0], z), conn(ids[1], [0.0; 3], z)));
        asm.mates.push(mate(m + 2, MateKind::Slider, conn(ids[1], [0.0, 0.0, 5.0], z), conn(ids[2], [0.0; 3], z)));
        asm.mates.push(mate(m + 3, MateKind::Revolute, conn(ids[2], [0.0, 0.0, 5.0], z), conn(ids[3], [0.0; 3], z)));
        asm.mates.push(mate(m + 4, MateKind::Parallel, conn(ids[3], [0.0; 3], z), conn(ids[1], [0.0; 3], z)));
        asm.mates.push(mate(m + 5, MateKind::Parallel, conn(ids[2], [0.0; 3], z), conn(ids[0], [0.0; 3], z)));
    }
    assert_eq!(asm.instances.len(), 101);
    assert_eq!(asm.mates.len(), 150);
    let t = std::time::Instant::now();
    let sol = solve(&asm);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    assert!(sol.converged, "{}", sol.residual);
    // 3 libres por módulo: la bisagra a la base, el deslizante y la bisagra de arriba
    assert_eq!(sol.dof, 75);
    eprintln!("100 piezas / 150 uniones: {ms:.1} ms");
    // En debug es varias veces más lento: la meta (100 ms) es para release
    let limit = if cfg!(debug_assertions) { 2000.0 } else { 100.0 };
    assert!(ms < limit, "tardó {ms:.1} ms");
}

#[test]
fn en8_patrones_grupos_espejo_y_suprimidas() {
    // Fila de 3 copias cada 15 mm a lo largo de X de la original: siguen a la original
    let mut asm = pair(MateKind::Fastened);
    for id in 3..6 {
        asm.instances.push(inst(id, [0.0; 3], false));
    }
    asm.patterns.push(InstancePattern { id: 9, source: 2, kind: PatternKind::Linear { dir: [1.0, 0.0, 0.0], step: 15.0 }, copies: vec![3, 4, 5] });
    let sol = solve(&asm);
    assert!(sol.converged);
    for (k, id) in [3, 4, 5].iter().enumerate() {
        let o = apply(pose(&sol, *id), [0.0; 3]);
        assert_relative_eq!(o[0], 15.0 * (k + 1) as f64, epsilon = 1e-6);
        assert_relative_eq!(o[2], 10.0, epsilon = 1e-6);
    }
    // En círculo: 4 copias cada 90° alrededor de Z por (10, 0, 0) de la original
    let mut asm = pair(MateKind::Fastened);
    asm.instances.push(inst(3, [0.0; 3], false));
    asm.patterns.push(InstancePattern { id: 9, source: 2, kind: PatternKind::Circular { origin: [10.0, 0.0, 0.0], axis: [0.0, 0.0, 1.0], angle: 90.0 }, copies: vec![3] });
    let sol = solve(&asm);
    let o = apply(pose(&sol, 3), [0.0; 3]);
    assert_relative_eq!(o[0], 10.0, epsilon = 1e-6);
    assert_relative_eq!(o[1], -10.0, epsilon = 1e-6);
    // Sobre agujeros de A: corrimientos en las coordenadas de A
    let mut asm = pair(MateKind::Fastened);
    asm.instances.push(inst(3, [0.0; 3], false));
    asm.patterns.push(InstancePattern { id: 9, source: 2, kind: PatternKind::Points { reference: 1, offsets: vec![[0.0, 20.0, 0.0]] }, copies: vec![3] });
    let sol = solve(&asm);
    let o = apply(pose(&sol, 3), [0.0; 3]);
    assert_relative_eq!(o[1], 20.0, epsilon = 1e-6);
    assert_relative_eq!(o[2], 10.0, epsilon = 1e-6);
    // Grupo: la 3 viaja con la 2 sin uniones, guardando su lugar
    let mut asm = pair(MateKind::Fastened);
    asm.instances.push(inst(3, [25.0, 3.0, 7.0], false));
    asm.groups.push(InstanceGroup { id: 9, name: "g".into(), members: vec![2, 3] });
    let sol = solve(&asm);
    assert!(sol.converged);
    let (a, b) = (apply(pose(&sol, 2), [0.0; 3]), apply(pose(&sol, 3), [0.0; 3]));
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    assert_relative_eq!((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt(), 5.0, epsilon = 1e-6);
    assert_eq!(sol.dof, 0);
    // Suprimida: su unión no cuenta y no se mueve
    let mut asm = pair(MateKind::Fastened);
    asm.instances[1].suppressed = true;
    let sol = solve(&asm);
    assert_eq!(sol.dof, 0);
    assert_eq!(sol.poses[1].1, [20.0, 3.0, 7.0]);
    // Espejo: el conector se refleja con la pieza
    let i = Instance { mirror: Some([1.0, 0.0, 0.0]), ..inst(1, [0.0; 3], false) };
    assert_eq!(i.mirror_point([3.0, 1.0, 2.0]), [-3.0, 1.0, 2.0]);
    let p = Pose::about([0.0; 3], [0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2);
    let v = p.rotation_vector();
    assert_relative_eq!(v[2], std::f64::consts::FRAC_PI_2, epsilon = 1e-12);
}

#[test]
fn en10_en11_pasos_de_animacion_y_explosion() {
    let mut asm = pair(MateKind::Revolute);
    asm.instances.push(inst(3, [50.0, 0.0, 0.0], false));
    asm.mates.push(mate(2, MateKind::Slider, conn(1, [50.0, 0.0, 0.0], [0.0, 0.0, 1.0]), conn(3, [0.0; 3], [0.0, 0.0, 1.0])));
    let steps = vec![
        MotionStep { mate: 1, var: SweepVar::Angle, from: 0.0, to: 90.0, seconds: 1.0 },
        MotionStep { mate: 2, var: SweepVar::Distance, from: 0.0, to: 10.0, seconds: 0.5 },
    ];
    let frames = sweep_steps(&asm, &steps, 10.0).unwrap();
    assert_eq!(frames.len(), 10 + 5 - 1);
    // Al final del segundo paso, la bisagra sigue abierta a 90° y el deslizante a 10 mm
    let last = frames.last().unwrap();
    assert_relative_eq!(assembly::measure(&asm, last, 1).unwrap().0.abs(), 90.0, epsilon = 1e-4);
    assert_relative_eq!(pose(last, 3)[2][3], 10.0, epsilon = 1e-6);
    asm.explode = vec![ExplodeStep { instances: vec![2], dir: [0.0, 0.0, 2.0], distance: 30.0 }, ExplodeStep { instances: vec![2, 3], dir: [1.0, 0.0, 0.0], distance: 10.0 }];
    let off = explode_offsets(&asm, 1.5);
    assert_eq!(off, vec![(2, [5.0, 0.0, 30.0]), (3, [5.0, 0.0, 0.0])]);
}
