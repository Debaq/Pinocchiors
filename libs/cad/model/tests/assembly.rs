//! Ensambles: solver de relaciones, grados libres e interferencias.

use approx::assert_relative_eq;
use cad_model::assembly::{instance_matrix, instance_shapes, interferences, solve};
use cad_model::*;

fn occt() -> bool {
    occt::available()
}

fn inst(id: u32, position: P3, fixed: bool) -> Instance {
    Instance { id, studio: 0, configuration: None, part: PartId { feature: FeatureId(0), index: 0 }, name: format!("I{id}"), position, rotation: [0.3, -0.2, 0.5], fixed }
}

/// Arriba de A (cubo de 10 desde el origen) y abajo de B, enfrentados.
fn stack(kind: MateKind) -> Assembly {
    let mut a = inst(1, [0.0; 3], true);
    a.rotation = [0.0; 3];
    let b = inst(2, [30.0, -12.0, 4.0], false);
    let mate = Mate {
        id: 1,
        name: String::new(),
        kind,
        a: Connector { instance: 1, origin: [5.0, 5.0, 10.0], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0] },
        b: Connector { instance: 2, origin: [5.0, 5.0, 0.0], z: [0.0, 0.0, -1.0], x: [1.0, 0.0, 0.0] },
        flip: true,
        angle: None,
        distance: None,
    };
    Assembly { instances: vec![a, b], mates: vec![mate], next_id: 3, ..Default::default() }
}

fn apply(m: [[f64; 4]; 3], p: P3) -> P3 {
    [0, 1, 2].map(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3])
}

#[test]
fn hinge_leaves_one_degree_and_angle_drives_it() {
    let asm = stack(MateKind::Revolute);
    let sol = solve(&asm);
    assert!(sol.converged, "{sol:?}");
    assert_eq!(sol.dof, 1, "bisagra: un giro libre");
    let (_, p, r) = sol.poses[1];
    let bottom = apply(instance_matrix(p, r), [5.0, 5.0, 0.0]);
    for (g, w) in bottom.iter().zip([5.0, 5.0, 10.0]) {
        assert_relative_eq!(*g, w, epsilon = 1e-7);
    }
    // Con ángulo impuesto: 0 grados libres y X de B girado 90°
    let mut asm = stack(MateKind::Revolute);
    asm.mates[0].angle = Some(90.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    assert_eq!(sol.dof, 0);
    let (_, p, r) = sol.poses[1];
    let m = instance_matrix(p, r);
    let x = [m[0][0], m[1][0], m[2][0]];
    assert_relative_eq!(x[1], 1.0, epsilon = 1e-7);
}

#[test]
fn fastened_slider_and_conflicts() {
    let sol = solve(&stack(MateKind::Fastened));
    assert!(sol.converged);
    assert_eq!(sol.dof, 0);
    // Deslizante: 1 grado libre; con distancia, ninguno y B a 7 mm sobre A
    let sol = solve(&stack(MateKind::Slider));
    assert_eq!(sol.dof, 1);
    let mut asm = stack(MateKind::Slider);
    asm.mates[0].distance = Some(7.0);
    let sol = solve(&asm);
    assert!(sol.converged);
    let (_, p, r) = sol.poses[1];
    assert_relative_eq!(apply(instance_matrix(p, r), [5.0, 5.0, 0.0])[2], 17.0, epsilon = 1e-7);
    // Dos fijas que se contradicen: no converge
    let mut asm = stack(MateKind::Fastened);
    let mut other = asm.mates[0].clone();
    other.id = 2;
    other.a.origin = [5.0, 5.0, 30.0];
    asm.mates.push(other);
    assert!(!solve(&asm).converged);
}

#[test]
fn instances_and_interference() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 10.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    // Dos copias de la misma pieza, la segunda corrida 9 mm: chocan 1 × 10 × 10
    let mut a = inst(1, [0.0; 3], true);
    a.rotation = [0.0; 3];
    let mut b = inst(2, [9.0, 0.0, 0.0], true);
    b.rotation = [0.0; 3];
    let asm = Assembly { instances: vec![a, b], mates: vec![], next_id: 3, ..Default::default() };
    let sol = solve(&asm);
    let shapes = instance_shapes(&ev, &asm, &sol);
    assert_eq!(shapes.len(), 2);
    let hits = interferences(&shapes);
    assert_eq!(hits.len(), 1);
    assert_relative_eq!(hits[0].2, 100.0, max_relative = 1e-6);
}

#[test]
fn tight_gaps_report_pairs_closer_than_the_clearance() {
    if !occt() {
        return;
    }
    let mut doc = Document::new();
    doc.add(FeatureKind::Primitive(Primitive {
        shape: PrimitiveShape::Box { dx: 10.0, dy: 10.0, dz: 10.0, centered: false, centered_z: false },
        origin: [0.0; 3],
        z: [0.0, 0.0, 1.0],
        x: [1.0, 0.0, 0.0],
        op: BodyOp::Join,
        link: None,
    }));
    let ev = doc.evaluate();
    let mk = |id: u32, x: f64| {
        let mut i = inst(id, [x, 0.0, 0.0], true);
        i.rotation = [0.0; 3];
        i
    };
    // A en 0..10; B a 0,3 mm; C a 0,8 mm de B (más holgada); D choca con C
    let asm = Assembly { instances: vec![mk(1, 0.0), mk(2, 10.3), mk(3, 21.1), mk(4, 30.0)], mates: vec![], next_id: 5, ..Default::default() };
    let sol = solve(&asm);
    let shapes = instance_shapes(&ev, &asm, &sol);
    let gaps = cad_model::assembly::tight_gaps(&ev, &asm, &shapes, 0.4);
    // Solo A–B (0,3 < 0,4); B–C hay 0,8; C–D se pisan (ya salen en los choques)
    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert_eq!((gaps[0].0, gaps[0].1), (1, 2));
    assert_relative_eq!(gaps[0].2, 0.3, epsilon = 1e-6);
    // Con una holgura mayor entran las dos separaciones
    assert_eq!(cad_model::assembly::tight_gaps(&ev, &asm, &shapes, 1.0).len(), 2);
}

#[test]
fn spherical_parallel_and_pin_slot_leave_their_degrees() {
    // Rótula: los orígenes coinciden y quedan los 3 giros de B
    let sol = solve(&stack(MateKind::Spherical));
    assert!(sol.converged, "{sol:?}");
    assert_eq!(sol.dof, 3, "rótula: 3 giros libres");
    let (_, p, r) = sol.poses[1];
    let o = apply(instance_matrix(p, r), [5.0, 5.0, 0.0]);
    for (g, w) in o.iter().zip([5.0, 5.0, 10.0]) {
        assert_relative_eq!(*g, w, epsilon = 1e-7);
    }
    // Paralela: solo ata las direcciones (3 traslaciones + 1 giro libres)
    let sol = solve(&stack(MateKind::Parallel));
    assert!(sol.converged, "{sol:?}");
    assert_eq!(sol.dof, 4);
    // Pasador en ranura: corre por X de A y gira alrededor de Z: 2 grados; con distancia, 1
    let sol = solve(&stack(MateKind::PinSlot));
    assert!(sol.converged, "{sol:?}");
    assert_eq!(sol.dof, 2);
    let mut asm = stack(MateKind::PinSlot);
    asm.mates[0].distance = Some(6.0);
    let sol = solve(&asm);
    assert!(sol.converged, "{sol:?}");
    assert_eq!(sol.dof, 1);
    let (_, p, r) = sol.poses[1];
    let o = apply(instance_matrix(p, r), [5.0, 5.0, 0.0]);
    for (g, w) in o.iter().zip([11.0, 5.0, 10.0]) {
        assert_relative_eq!(*g, w, epsilon = 1e-7);
    }
}

#[test]
fn sweeping_a_hinge_moves_the_part_continuously() {
    use cad_model::assembly::{sweep, SweepVar};
    let asm = stack(MateKind::Revolute);
    let frames = sweep(&asm, 1, SweepVar::Angle, 0.0, 90.0, 4).unwrap();
    assert_eq!(frames.len(), 4);
    for (k, sol) in frames.iter().enumerate() {
        let (_, p, r) = sol.poses[1];
        let m = instance_matrix(p, r);
        // X de B girado k·30° alrededor de Z
        let want = (30.0 * k as f64).to_radians();
        assert_relative_eq!(m[1][0], want.sin(), epsilon = 1e-6);
        assert_relative_eq!(m[0][0], want.cos(), epsilon = 1e-6);
    }
    // Una deslizante no se anima por ángulo
    assert!(sweep(&stack(MateKind::Slider), 1, SweepVar::Angle, 0.0, 90.0, 4).is_err());
    let slid = sweep(&stack(MateKind::Slider), 1, SweepVar::Distance, 0.0, 6.0, 3).unwrap();
    let (_, p, r) = slid[2].poses[1];
    assert_relative_eq!(apply(instance_matrix(p, r), [5.0, 5.0, 0.0])[2], 16.0, epsilon = 1e-7);
}
