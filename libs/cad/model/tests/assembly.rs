//! Ensambles: solver de relaciones, grados libres e interferencias.

use approx::assert_relative_eq;
use cad_model::assembly::{instance_matrix, instance_shapes, interferences, solve};
use cad_model::*;

fn occt() -> bool {
    occt::available()
}

fn inst(id: u32, position: P3, fixed: bool) -> Instance {
    Instance { id, part: PartId { feature: FeatureId(0), index: 0 }, name: format!("I{id}"), position, rotation: [0.3, -0.2, 0.5], fixed }
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
    Assembly { instances: vec![a, b], mates: vec![mate], next_id: 3 }
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
    let asm = Assembly { instances: vec![a, b], mates: vec![], next_id: 3 };
    let sol = solve(&asm);
    let shapes = instance_shapes(&ev, &asm, &sol);
    assert_eq!(shapes.len(), 2);
    let hits = interferences(&shapes);
    assert_eq!(hits.len(), 1);
    assert_relative_eq!(hits[0].2, 100.0, max_relative = 1e-6);
}
