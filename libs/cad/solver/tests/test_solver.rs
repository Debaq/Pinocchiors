#![allow(clippy::needless_range_loop, clippy::approx_constant)]

use approx::assert_relative_eq;
use cad_solver::constraint::Constraint;
use cad_solver::solver::{solve, SolverParams};
use cad_solver::system::ConstraintSystem;
use cad_solver::types::{Point2, SolveStatus};

fn default_params() -> SolverParams {
    SolverParams::default()
}

#[test]
fn test_fixed_point() {
    let mut sys = ConstraintSystem::new();
    let p = sys.add_point(1.0, 2.0);
    sys.add_constraint(Constraint::Fixed {
        p_idx: p,
        position: Point2::new(3.0, 4.0),
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::Converged);
    assert_eq!(result.dof, 0);
    assert_relative_eq!(result.points[p].x(), 3.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p].y(), 4.0, epsilon = 1e-8);
}

#[test]
fn test_distance_constraint() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(3.0, 0.0);

    // Fijar p0 en origen
    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    // Distancia p0-p1 = 5
    sys.add_constraint(Constraint::Distance {
        p1_idx: p0,
        p2_idx: p1,
        distance: 5.0,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::UnderConstrained);

    // Verificar que la distancia es 5
    let dx = result.points[p1].x() - result.points[p0].x();
    let dy = result.points[p1].y() - result.points[p0].y();
    let dist = (dx * dx + dy * dy).sqrt();
    assert_relative_eq!(dist, 5.0, epsilon = 1e-8);
}

#[test]
fn test_horizontal_constraint() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(5.0, 3.0);

    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Horizontal {
        p1_idx: p0,
        p2_idx: p1,
    });
    sys.add_constraint(Constraint::Distance {
        p1_idx: p0,
        p2_idx: p1,
        distance: 5.0,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::Converged);
    assert_relative_eq!(result.points[p1].y(), 0.0, epsilon = 1e-8);
}

#[test]
fn test_perpendicular_lines() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(5.0, 0.0);
    let p2 = sys.add_point(0.0, 0.0);
    let p3 = sys.add_point(1.0, 2.0);

    // Fijar todos menos p3
    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p1,
        position: Point2::new(5.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p2,
        position: Point2::new(0.0, 0.0),
    });
    // p2-p3 perpendicular a p0-p1
    sys.add_constraint(Constraint::Perpendicular {
        l1_p1: p0,
        l1_p2: p1,
        l2_p1: p2,
        l2_p2: p3,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    // p3.x debe ser 0 (línea vertical = perpendicular a horizontal)
    assert_relative_eq!(result.points[p3].x(), 0.0, epsilon = 1e-6);
}

#[test]
fn test_rectangle() {
    // Construir un rectángulo 4x3 completamente restringido
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(4.5, 0.5);
    let p2 = sys.add_point(4.5, 3.5);
    let p3 = sys.add_point(0.5, 3.5);

    // Fijar esquina inferior izquierda
    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });

    // Lados horizontales
    sys.add_constraint(Constraint::Horizontal {
        p1_idx: p0,
        p2_idx: p1,
    });
    sys.add_constraint(Constraint::Horizontal {
        p1_idx: p3,
        p2_idx: p2,
    });

    // Lados verticales
    sys.add_constraint(Constraint::Vertical {
        p1_idx: p0,
        p2_idx: p3,
    });
    sys.add_constraint(Constraint::Vertical {
        p1_idx: p1,
        p2_idx: p2,
    });

    // Dimensiones: ancho=4, alto=3
    sys.add_constraint(Constraint::HorizontalDist {
        p1_idx: p1,
        p2_idx: p0,
        distance: 4.0,
    });
    sys.add_constraint(Constraint::VerticalDist {
        p1_idx: p3,
        p2_idx: p0,
        distance: 3.0,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::Converged);
    assert_eq!(result.dof, 0);

    assert_relative_eq!(result.points[p0].x(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p0].y(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p1].x(), 4.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p1].y(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p2].x(), 4.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p2].y(), 3.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p3].x(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p3].y(), 3.0, epsilon = 1e-8);
}

#[test]
fn test_parallel_lines() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(1.0, 1.0);
    let p2 = sys.add_point(0.0, 2.0);
    let p3 = sys.add_point(1.5, 3.5);

    // Fijar p0, p1, p2
    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p1,
        position: Point2::new(1.0, 1.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p2,
        position: Point2::new(0.0, 2.0),
    });

    // Línea p2-p3 paralela a p0-p1
    sys.add_constraint(Constraint::Parallel {
        l1_p1: p0,
        l1_p2: p1,
        l2_p1: p2,
        l2_p2: p3,
    });

    let result = solve(&mut sys, &default_params()).unwrap();

    // p3 - p2 debe ser paralelo a p1 - p0 = (1, 1)
    // Es decir, (p3.x - 0) * 1 - (p3.y - 2) * 1 = 0
    // p3.x - p3.y + 2 = 0
    let d2 = (
        result.points[p3].x() - result.points[p2].x(),
        result.points[p3].y() - result.points[p2].y(),
    );
    // Cross product con (1,1) debe ser ~0
    let cross = d2.0 * 1.0 - d2.1 * 1.0;
    assert_relative_eq!(cross, 0.0, epsilon = 1e-6);
}

#[test]
fn test_empty_system() {
    let mut sys = ConstraintSystem::new();
    sys.add_point(1.0, 2.0);

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::UnderConstrained);
    assert_eq!(result.dof, 2);
}

#[test]
fn test_coincident() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(3.0, 4.0);

    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Coincident {
        p1_idx: p0,
        p2_idx: p1,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::Converged);
    assert_relative_eq!(result.points[p1].x(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p1].y(), 0.0, epsilon = 1e-8);
}

#[test]
fn test_point_on_line() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(4.0, 0.0);
    let p2 = sys.add_point(2.0, 3.0); // punto fuera de la línea

    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p1,
        position: Point2::new(4.0, 0.0),
    });
    sys.add_constraint(Constraint::PointOnLine {
        p_idx: p2,
        line_p1: p0,
        line_p2: p1,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    // p2 debe tener y=0 (sobre la línea horizontal)
    assert_relative_eq!(result.points[p2].y(), 0.0, epsilon = 1e-6);
}

#[test]
fn test_midpoint() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(6.0, 4.0);
    let p_mid = sys.add_point(1.0, 1.0);

    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p1,
        position: Point2::new(6.0, 4.0),
    });
    sys.add_constraint(Constraint::Midpoint {
        p_idx: p_mid,
        line_p1: p0,
        line_p2: p1,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_relative_eq!(result.points[p_mid].x(), 3.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p_mid].y(), 2.0, epsilon = 1e-8);
}

#[test]
fn test_equal_length() {
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(3.0, 0.0);
    let p2 = sys.add_point(0.0, 1.0);
    let p3 = sys.add_point(0.0, 5.0);

    // Fijar p0, p1, p2
    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p1,
        position: Point2::new(3.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p2,
        position: Point2::new(0.0, 1.0),
    });
    // Línea p2-p3 vertical
    sys.add_constraint(Constraint::Vertical {
        p1_idx: p2,
        p2_idx: p3,
    });
    // Largo igual: |p0-p1| == |p2-p3|
    sys.add_constraint(Constraint::EqualLength {
        l1_p1: p0,
        l1_p2: p1,
        l2_p1: p2,
        l2_p2: p3,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    // |p0-p1| = 3, |p2-p3| debe ser 3 también
    let d = (result.points[p3].y() - result.points[p2].y()).abs();
    assert_relative_eq!(d, 3.0, epsilon = 1e-6);
}

#[test]
fn test_triangle_with_constraints() {
    // Triángulo con base horizontal de largo 5, altura 3
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(5.5, 0.5);
    let p2 = sys.add_point(2.0, 3.5);

    // Fijar p0
    sys.add_constraint(Constraint::Fixed {
        p_idx: p0,
        position: Point2::new(0.0, 0.0),
    });
    // Base horizontal
    sys.add_constraint(Constraint::Horizontal {
        p1_idx: p0,
        p2_idx: p1,
    });
    // Largo base = 5
    sys.add_constraint(Constraint::HorizontalDist {
        p1_idx: p1,
        p2_idx: p0,
        distance: 5.0,
    });
    // Vértice superior en x=2.5 (punto medio horizontal de la base)
    sys.add_constraint(Constraint::HorizontalDist {
        p1_idx: p2,
        p2_idx: p0,
        distance: 2.5,
    });
    // Altura = 3
    sys.add_constraint(Constraint::VerticalDist {
        p1_idx: p2,
        p2_idx: p0,
        distance: 3.0,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::Converged);
    assert_eq!(result.dof, 0);

    assert_relative_eq!(result.points[p0].x(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p0].y(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p1].x(), 5.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p1].y(), 0.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[p2].x(), 2.5, epsilon = 1e-8);
    assert_relative_eq!(result.points[p2].y(), 3.0, epsilon = 1e-8);
}

#[test]
fn test_angle_constraint() {
    // Dos líneas desde el origen, una horizontal, la otra a 45°
    let mut sys = ConstraintSystem::new();
    let origin = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(5.0, 0.0);
    let p2 = sys.add_point(3.0, 1.0);

    sys.add_constraint(Constraint::Fixed {
        p_idx: origin,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: p1,
        position: Point2::new(5.0, 0.0),
    });
    // Distancia del segundo brazo = 4
    sys.add_constraint(Constraint::Distance {
        p1_idx: origin,
        p2_idx: p2,
        distance: 4.0,
    });
    // Ángulo de 45° entre las dos líneas
    sys.add_constraint(Constraint::Angle {
        l1_p1: origin,
        l1_p2: p1,
        l2_p1: origin,
        l2_p2: p2,
        angle_rad: std::f64::consts::FRAC_PI_4,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::Converged);

    // p2 debe estar a 45° → (4*cos45, 4*sin45) ≈ (2.828, 2.828)
    let expected_x = 4.0 * std::f64::consts::FRAC_PI_4.cos();
    let expected_y = 4.0 * std::f64::consts::FRAC_PI_4.sin();
    assert_relative_eq!(result.points[p2].x(), expected_x, epsilon = 1e-6);
    assert_relative_eq!(result.points[p2].y(), expected_y, epsilon = 1e-6);
}

#[test]
fn test_symmetric_constraint() {
    // Dos puntos simétricos respecto al eje Y (línea vertical x=0)
    let mut sys = ConstraintSystem::new();
    let line_p1 = sys.add_point(0.0, 0.0);
    let line_p2 = sys.add_point(0.0, 5.0);
    let p1 = sys.add_point(3.0, 2.0);
    let p2 = sys.add_point(-1.0, 2.5);

    // Fijar la línea de simetría
    sys.add_constraint(Constraint::Fixed {
        p_idx: line_p1,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Fixed {
        p_idx: line_p2,
        position: Point2::new(0.0, 5.0),
    });
    // Fijar p1
    sys.add_constraint(Constraint::Fixed {
        p_idx: p1,
        position: Point2::new(3.0, 2.0),
    });
    // p2 simétrico a p1 respecto a la línea vertical
    sys.add_constraint(Constraint::Symmetric {
        p1_idx: p1,
        p2_idx: p2,
        line_p1,
        line_p2,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    // p2 debe ser (-3, 2) — reflejo de (3,2) respecto a x=0
    assert_relative_eq!(result.points[p2].x(), -3.0, epsilon = 1e-6);
    assert_relative_eq!(result.points[p2].y(), 2.0, epsilon = 1e-6);
}

#[test]
fn test_tangent_line_circle() {
    // Línea horizontal tangente a círculo de radio 2 centrado en (0,0)
    let mut sys = ConstraintSystem::new();
    let center = sys.add_point(0.0, 0.0);
    let lp1 = sys.add_point(-5.0, 2.5);
    let lp2 = sys.add_point(5.0, 2.5);

    sys.add_constraint(Constraint::Fixed {
        p_idx: center,
        position: Point2::new(0.0, 0.0),
    });
    // Fijar solo x de cada punto, y libre
    sys.add_constraint(Constraint::HorizontalDist {
        p1_idx: lp1,
        p2_idx: center,
        distance: -5.0,
    });
    sys.add_constraint(Constraint::HorizontalDist {
        p1_idx: lp2,
        p2_idx: center,
        distance: 5.0,
    });
    // Línea horizontal
    sys.add_constraint(Constraint::Horizontal {
        p1_idx: lp1,
        p2_idx: lp2,
    });
    // Tangente al círculo de radio 2
    sys.add_constraint(Constraint::TangentLineCircle {
        line_p1: lp1,
        line_p2: lp2,
        center_idx: center,
        radius: 2.0,
    });

    let params = SolverParams {
        max_iterations: 200,
        tolerance: 1e-5,
        ..SolverParams::default()
    };
    let result = solve(&mut sys, &params).unwrap();
    // Línea horizontal a distancia 2 del origen → |y| = 2
    let y = result.points[lp1].y();
    assert_relative_eq!(y.abs(), 2.0, epsilon = 1e-3);
}

#[test]
fn test_point_on_circle() {
    let mut sys = ConstraintSystem::new();
    let center = sys.add_point(0.0, 0.0);
    let p = sys.add_point(3.5, 1.0);

    sys.add_constraint(Constraint::Fixed {
        p_idx: center,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::PointOnCircle {
        p_idx: p,
        center_idx: center,
        radius: 3.0,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    let dx = result.points[p].x();
    let dy = result.points[p].y();
    let dist = (dx * dx + dy * dy).sqrt();
    assert_relative_eq!(dist, 3.0, epsilon = 1e-8);
}

#[test]
fn test_radius_constraint() {
    let mut sys = ConstraintSystem::new();
    let center = sys.add_point(1.0, 1.0);
    let edge = sys.add_point(4.0, 1.0);

    sys.add_constraint(Constraint::Fixed {
        p_idx: center,
        position: Point2::new(1.0, 1.0),
    });
    sys.add_constraint(Constraint::Horizontal {
        p1_idx: center,
        p2_idx: edge,
    });
    sys.add_constraint(Constraint::Radius {
        center_idx: center,
        p_on_circle: edge,
        radius: 5.0,
    });

    let result = solve(&mut sys, &default_params()).unwrap();
    assert_eq!(result.status, SolveStatus::Converged);
    assert_relative_eq!(result.points[edge].x(), 6.0, epsilon = 1e-8);
    assert_relative_eq!(result.points[edge].y(), 1.0, epsilon = 1e-8);
}

#[test]
fn test_decompose_independent_systems() {
    // Dos rectángulos completamente independientes
    let mut sys = ConstraintSystem::new();

    // Rectángulo 1: puntos 0-3
    let r1_p0 = sys.add_point(0.0, 0.0);
    let r1_p1 = sys.add_point(3.5, 0.5);
    let r1_p2 = sys.add_point(3.5, 2.5);
    let r1_p3 = sys.add_point(0.5, 2.5);

    sys.add_constraint(Constraint::Fixed {
        p_idx: r1_p0,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::Horizontal { p1_idx: r1_p0, p2_idx: r1_p1 });
    sys.add_constraint(Constraint::Vertical { p1_idx: r1_p1, p2_idx: r1_p2 });
    sys.add_constraint(Constraint::Horizontal { p1_idx: r1_p3, p2_idx: r1_p2 });
    sys.add_constraint(Constraint::Vertical { p1_idx: r1_p0, p2_idx: r1_p3 });
    sys.add_constraint(Constraint::HorizontalDist { p1_idx: r1_p1, p2_idx: r1_p0, distance: 3.0 });
    sys.add_constraint(Constraint::VerticalDist { p1_idx: r1_p3, p2_idx: r1_p0, distance: 2.0 });

    // Rectángulo 2: puntos 4-7 (completamente separado)
    let r2_p0 = sys.add_point(10.0, 10.0);
    let r2_p1 = sys.add_point(15.5, 10.5);
    let r2_p2 = sys.add_point(15.5, 14.5);
    let r2_p3 = sys.add_point(10.5, 14.5);

    sys.add_constraint(Constraint::Fixed {
        p_idx: r2_p0,
        position: Point2::new(10.0, 10.0),
    });
    sys.add_constraint(Constraint::Horizontal { p1_idx: r2_p0, p2_idx: r2_p1 });
    sys.add_constraint(Constraint::Vertical { p1_idx: r2_p1, p2_idx: r2_p2 });
    sys.add_constraint(Constraint::Horizontal { p1_idx: r2_p3, p2_idx: r2_p2 });
    sys.add_constraint(Constraint::Vertical { p1_idx: r2_p0, p2_idx: r2_p3 });
    sys.add_constraint(Constraint::HorizontalDist { p1_idx: r2_p1, p2_idx: r2_p0, distance: 5.0 });
    sys.add_constraint(Constraint::VerticalDist { p1_idx: r2_p3, p2_idx: r2_p0, distance: 4.0 });

    // Verificar que decompose encuentra 2 sub-sistemas
    let subs = sys.decompose();
    assert_eq!(subs.len(), 2, "Debería descomponer en 2 sub-sistemas");
    println!("Sub-sistema 1: {} puntos, {} constraints", subs[0].points.len(), subs[0].constraints.len());
    println!("Sub-sistema 2: {} puntos, {} constraints", subs[1].points.len(), subs[1].constraints.len());

    // Resolver — debe funcionar con decomposición
    let start = std::time::Instant::now();
    let result = solve(&mut sys, &default_params()).unwrap();
    let elapsed = start.elapsed();

    println!("Decomposed solve: {:.2}ms, {} iter", elapsed.as_secs_f64() * 1000.0, result.iterations);

    // Verificar rectángulo 1: 3x2 en (0,0)
    assert_relative_eq!(result.points[r1_p1].x(), 3.0, epsilon = 1e-6);
    assert_relative_eq!(result.points[r1_p3].y(), 2.0, epsilon = 1e-6);

    // Verificar rectángulo 2: 5x4 en (10,10)
    assert_relative_eq!(result.points[r2_p1].x(), 15.0, epsilon = 1e-6);
    assert_relative_eq!(result.points[r2_p3].y(), 14.0, epsilon = 1e-6);
}

#[test]
fn test_large_sketch_grid() {
    // Grid 10x10 de rectángulos conectados — 121 puntos, ~220 constraints
    let mut sys = ConstraintSystem::new();
    let n = 10;
    let mut grid = vec![vec![0usize; n + 1]; n + 1];

    // Crear puntos con posiciones aproximadas
    for i in 0..=n {
        for j in 0..=n {
            grid[i][j] = sys.add_point(
                i as f64 * 1.1 + 0.05 * (j as f64), // ligeramente offset
                j as f64 * 1.1 + 0.05 * (i as f64),
            );
        }
    }

    // Fijar esquina
    sys.add_constraint(Constraint::Fixed {
        p_idx: grid[0][0],
        position: Point2::new(0.0, 0.0),
    });

    // Horizontal: todas las filas
    for i in 0..=n {
        for j in 0..n {
            sys.add_constraint(Constraint::Horizontal {
                p1_idx: grid[i][j],
                p2_idx: grid[i][j + 1],
            });
        }
    }

    // Vertical: todas las columnas
    for j in 0..=n {
        for i in 0..n {
            sys.add_constraint(Constraint::Vertical {
                p1_idx: grid[i][j],
                p2_idx: grid[i + 1][j],
            });
        }
    }

    // Distancias horizontales = 1.0
    for j in 0..n {
        sys.add_constraint(Constraint::HorizontalDist {
            p1_idx: grid[0][j + 1],
            p2_idx: grid[0][j],
            distance: 1.0,
        });
    }

    // Distancias verticales = 1.0
    for i in 0..n {
        sys.add_constraint(Constraint::VerticalDist {
            p1_idx: grid[i + 1][0],
            p2_idx: grid[i][0],
            distance: 1.0,
        });
    }

    let start = std::time::Instant::now();
    let result = solve(&mut sys, &default_params()).unwrap();
    let elapsed = start.elapsed();

    println!(
        "Grid {}x{}: {} puntos, {} constraints, {} iteraciones, {:.2}ms, sparsity={:.1}%",
        n, n,
        sys.points.len(),
        sys.constraints.len(),
        result.iterations,
        elapsed.as_secs_f64() * 1000.0,
        sys.sparsity_ratio() * 100.0,
    );

    // Verificar que converge
    assert!(
        result.status == SolveStatus::Converged || result.status == SolveStatus::UnderConstrained,
        "Status: {:?}, residual: {}",
        result.status, result.residual
    );

    // Verificar grid correcto: punto (5,5) debe estar en ~(5, 5)
    assert_relative_eq!(result.points[grid[5][5]].x(), 5.0, epsilon = 0.1);
    assert_relative_eq!(result.points[grid[5][5]].y(), 5.0, epsilon = 0.1);
}

#[test]
fn test_tangent_convergence_improved() {
    // Test que tangent ahora converge con tolerancia default (antes fallaba)
    let mut sys = ConstraintSystem::new();
    let center = sys.add_point(0.0, 0.0);
    let lp1 = sys.add_point(-5.0, 2.5);
    let lp2 = sys.add_point(5.0, 2.5);

    sys.add_constraint(Constraint::Fixed {
        p_idx: center,
        position: Point2::new(0.0, 0.0),
    });
    sys.add_constraint(Constraint::HorizontalDist {
        p1_idx: lp1,
        p2_idx: center,
        distance: -5.0,
    });
    sys.add_constraint(Constraint::HorizontalDist {
        p1_idx: lp2,
        p2_idx: center,
        distance: 5.0,
    });
    sys.add_constraint(Constraint::Horizontal {
        p1_idx: lp1,
        p2_idx: lp2,
    });
    sys.add_constraint(Constraint::TangentLineCircle {
        line_p1: lp1,
        line_p2: lp2,
        center_idx: center,
        radius: 2.0,
    });

    // Ahora debería converger con tolerancia DEFAULT (1e-10)
    let result = solve(&mut sys, &default_params()).unwrap();
    let y = result.points[lp1].y();
    assert_relative_eq!(y.abs(), 2.0, epsilon = 1e-6);
    println!("Tangent: {} iteraciones, residuo={:.2e}", result.iterations, result.residual);
}

#[test]
fn test_solve_drag_keeps_constraints_and_removes_temp() {
    use cad_solver::solve_drag;
    let mut sys = ConstraintSystem::new();
    let p0 = sys.add_point(0.0, 0.0);
    let p1 = sys.add_point(10.0, 0.0);
    sys.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(0.0, 0.0) });
    sys.add_constraint(Constraint::Distance { p1_idx: p0, p2_idx: p1, distance: 10.0 });
    let n = sys.constraints.len();

    // Arrastrar p1 hacia (0, 20): debe quedar a distancia 10 sobre ese eje
    let r = solve_drag(&mut sys, p1, Point2::new(0.0, 20.0)).unwrap();
    assert_eq!(sys.constraints.len(), n);
    let p = r.points[p1];
    assert_relative_eq!((p.x() * p.x() + p.y() * p.y()).sqrt(), 10.0, epsilon = 1e-4);
    assert!(p.y() > 9.0);

    assert!(solve_drag(&mut sys, 99, Point2::new(0.0, 0.0)).is_err());
    assert_eq!(sys.constraints.len(), n);
}

#[test]
fn test_free_points_follow_nullspace() {
    use cad_solver::diagnose;
    // Línea horizontal con un extremo fijo: el otro extremo se desliza en x
    // aunque lo toquen dos ecuaciones (horizontal + nada más en x)
    let mut sys = ConstraintSystem::new();
    let a = sys.add_point(0.0, 0.0);
    let b = sys.add_point(5.0, 0.0);
    sys.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    sys.add_constraint(Constraint::Horizontal { p1_idx: a, p2_idx: b });
    let d = diagnose(&sys);
    assert_eq!(d.dof_per_point[a].1, 0);
    assert_eq!(d.dof_per_point[b].1, 1);
    // Con el largo fijo, ya no se mueve
    sys.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 5.0 });
    let d = diagnose(&sys);
    assert_eq!(d.dof_per_point[b].1, 0);
}
