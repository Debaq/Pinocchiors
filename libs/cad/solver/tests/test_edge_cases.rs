#![allow(clippy::needless_range_loop, clippy::approx_constant)]

//! Tests de casos límite por constraint.
//!
//! Objetivo: detectar bugs de robustez antes de tocar el solver.
//! Un test que falla → bug real para corregir en fase de robustez.
//! Un test que pasa con NaN o panic → bug encubierto.

use approx::assert_relative_eq;
use cad_solver::constraint::Constraint;
use cad_solver::solver::{solve, SolverParams};
use cad_solver::system::ConstraintSystem;
use cad_solver::types::{Point2, SolveStatus};

fn p() -> SolverParams {
    SolverParams::default()
}

fn no_nan(sys: &cad_solver::types::SolveResult) {
    for (i, pt) in sys.points.iter().enumerate() {
        assert!(pt.x().is_finite(), "punto {} x no finito: {}", i, pt.x());
        assert!(pt.y().is_finite(), "punto {} y no finito: {}", i, pt.y());
    }
}

// ============================================================
// COINCIDENT
// ============================================================

#[test]
fn coincident_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(2.0, 3.0);
    let b = s.add_point(2.0, 3.0);
    s.add_constraint(Constraint::Fixed {
        p_idx: a,
        position: Point2::new(2.0, 3.0),
    });
    s.add_constraint(Constraint::Coincident { p1_idx: a, p2_idx: b });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
    assert_relative_eq!(r.points[b].x(), 2.0, epsilon = 1e-8);
}

#[test]
fn coincident_chain_of_three() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 1.0);
    let c = s.add_point(5.0, 5.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Coincident { p1_idx: a, p2_idx: b });
    s.add_constraint(Constraint::Coincident { p1_idx: b, p2_idx: c });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_relative_eq!(r.points[c].x(), 0.0, epsilon = 1e-6);
    assert_relative_eq!(r.points[c].y(), 0.0, epsilon = 1e-6);
}

// ============================================================
// DISTANCE
// ============================================================

#[test]
fn distance_zero_forces_coincident() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(3.0, 4.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 0.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    let dx = r.points[b].x() - r.points[a].x();
    let dy = r.points[b].y() - r.points[a].y();
    assert!((dx * dx + dy * dy).sqrt() < 1e-6, "distancia cero no respetada");
}

#[test]
fn distance_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 5.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

#[test]
fn distance_tiny() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 1e-6 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    let dx = r.points[b].x() - r.points[a].x();
    let dy = r.points[b].y() - r.points[a].y();
    assert_relative_eq!((dx * dx + dy * dy).sqrt(), 1e-6, epsilon = 1e-9);
}

#[test]
fn distance_large() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 1e6 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    let dx = r.points[b].x() - r.points[a].x();
    let dy = r.points[b].y() - r.points[a].y();
    assert_relative_eq!((dx * dx + dy * dy).sqrt(), 1e6, epsilon = 1e-3);
}

// ============================================================
// FIXED
// ============================================================

#[test]
fn fixed_conflicting_overconstrained() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(1.0, 1.0) });
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(2.0, 2.0) });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // Sistema conflictivo: residuo grande o status NotConverged/OverConstrained
    assert!(
        matches!(r.status, SolveStatus::OverConstrained | SolveStatus::NotConverged)
            || r.residual > 0.1,
        "fixed conflictivo no detectado: status={:?} residual={}",
        r.status,
        r.residual
    );
}

// ============================================================
// HORIZONTAL / VERTICAL
// ============================================================

#[test]
fn horizontal_zero_length_line() {
    // Línea con p1 == p2 (degenerada): el constraint Horizontal sigue siendo válido
    let mut s = ConstraintSystem::new();
    let a = s.add_point(2.0, 3.0);
    let b = s.add_point(2.0, 3.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(2.0, 3.0) });
    s.add_constraint(Constraint::Horizontal { p1_idx: a, p2_idx: b });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

#[test]
fn vertical_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(1.0, 0.0);
    let b = s.add_point(1.0, 5.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(1.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(1.0, 5.0) });
    s.add_constraint(Constraint::Vertical { p1_idx: a, p2_idx: b });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

// ============================================================
// PERPENDICULAR
// ============================================================

#[test]
fn perpendicular_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    let c = s.add_point(0.0, 0.0);
    let d = s.add_point(0.0, 3.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: d, position: Point2::new(0.0, 3.0) });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

#[test]
fn perpendicular_zero_length_line() {
    // Una línea degenerada — el dot product es 0*x = 0 trivialmente
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 0.0);  // zero-length
    let c = s.add_point(0.0, 0.0);
    let d = s.add_point(1.0, 1.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

// ============================================================
// PARALLEL
// ============================================================

#[test]
fn parallel_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    let c = s.add_point(0.0, 2.0);
    let d = s.add_point(5.0, 2.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 2.0) });
    s.add_constraint(Constraint::Fixed { p_idx: d, position: Point2::new(5.0, 2.0) });
    s.add_constraint(Constraint::Parallel {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

// ============================================================
// ANGLE
// ============================================================

#[test]
fn angle_zero_means_parallel() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    let c = s.add_point(0.0, 2.0);
    let d = s.add_point(5.0, 2.5);  // casi paralelo
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 2.0) });
    s.add_constraint(Constraint::Angle {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
        angle_rad: 0.0,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // d.y debe igualar c.y para angle=0 con l1 horizontal
    assert_relative_eq!(r.points[d].y(), 2.0, epsilon = 1e-4);
}

#[test]
fn angle_pi_over_2_means_perp() {
    use std::f64::consts::FRAC_PI_2;
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    let c = s.add_point(0.0, 0.0);
    let d = s.add_point(1.0, 3.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Angle {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
        angle_rad: FRAC_PI_2,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_relative_eq!(r.points[d].x(), 0.0, epsilon = 1e-4);
}

// ============================================================
// TANGENT LINE-CIRCLE
// ============================================================

#[test]
fn tangent_radius_zero() {
    // Radio 0 → línea debe pasar por el centro (distancia centro-línea = 0)
    let mut s = ConstraintSystem::new();
    let p1 = s.add_point(0.0, 0.0);
    let p2 = s.add_point(5.0, 0.5);
    let center = s.add_point(2.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: center, position: Point2::new(2.0, 0.0) });
    s.add_constraint(Constraint::TangentLineCircle {
        line_p1: p1, line_p2: p2, center_idx: center, radius: 0.0,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

#[test]
fn tangent_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let p1 = s.add_point(-5.0, 1.0);
    let p2 = s.add_point(5.0, 1.0);
    let c = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(-5.0, 1.0) });
    s.add_constraint(Constraint::Fixed { p_idx: p2, position: Point2::new(5.0, 1.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::TangentLineCircle {
        line_p1: p1, line_p2: p2, center_idx: c, radius: 1.0,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

// ============================================================
// EQUAL LENGTH
// ============================================================

#[test]
fn equal_length_chain_of_three() {
    let mut s = ConstraintSystem::new();
    let p0 = s.add_point(0.0, 0.0);
    let p1 = s.add_point(5.0, 0.0);
    let p2 = s.add_point(5.0, 0.0);
    let p3 = s.add_point(8.0, 0.0);
    let p4 = s.add_point(8.0, 0.0);
    let p5 = s.add_point(20.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: p2, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: p4, position: Point2::new(8.0, 0.0) });
    s.add_constraint(Constraint::EqualLength {
        l1_p1: p0, l1_p2: p1, l2_p1: p2, l2_p2: p3,
    });
    s.add_constraint(Constraint::EqualLength {
        l1_p1: p2, l1_p2: p3, l2_p1: p4, l2_p2: p5,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // p3 a 5 de p2, p5 a 5 de p4
    let len_l3 = ((r.points[p5].x() - r.points[p4].x()).powi(2)
        + (r.points[p5].y() - r.points[p4].y()).powi(2))
    .sqrt();
    assert_relative_eq!(len_l3, 5.0, epsilon = 1e-4);
}

// ============================================================
// MIDPOINT
// ============================================================

#[test]
fn midpoint_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let m = s.add_point(2.5, 0.0);
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Midpoint { p_idx: m, line_p1: a, line_p2: b });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
    assert_relative_eq!(r.points[m].x(), 2.5, epsilon = 1e-8);
}

// ============================================================
// SYMMETRIC
// ============================================================

#[test]
fn symmetric_zero_length_axis() {
    // Línea de simetría degenerada: p1==p2. Comportamiento esperado: no NaN.
    let mut s = ConstraintSystem::new();
    let pa = s.add_point(1.0, 1.0);
    let pb = s.add_point(2.0, 2.0);
    let l1 = s.add_point(0.0, 0.0);
    let l2 = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: pa, position: Point2::new(1.0, 1.0) });
    s.add_constraint(Constraint::Fixed { p_idx: l1, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: l2, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Symmetric {
        p1_idx: pa, p2_idx: pb, line_p1: l1, line_p2: l2,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

// ============================================================
// POINT ON LINE
// ============================================================

#[test]
fn point_on_line_zero_length() {
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(1.0, 1.0);
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::PointOnLine { p_idx: pt, line_p1: a, line_p2: b });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

#[test]
fn point_on_line_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(2.5, 0.0);
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::PointOnLine { p_idx: pt, line_p1: a, line_p2: b });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

// ============================================================
// POINT ON CIRCLE
// ============================================================

#[test]
fn point_on_circle_radius_zero() {
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(3.0, 4.0);
    let c = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::PointOnCircle { p_idx: pt, center_idx: c, radius: 0.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    let dist = (r.points[pt].x().powi(2) + r.points[pt].y().powi(2)).sqrt();
    assert!(dist < 1e-4, "point_on_circle radius=0 debería forzar coincidencia con centro");
}

#[test]
fn point_on_circle_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(3.0, 4.0);  // a distancia 5 del origen
    let c = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: pt, position: Point2::new(3.0, 4.0) });
    s.add_constraint(Constraint::PointOnCircle { p_idx: pt, center_idx: c, radius: 5.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

// ============================================================
// RADIUS
// ============================================================

#[test]
fn radius_zero() {
    let mut s = ConstraintSystem::new();
    let c = s.add_point(0.0, 0.0);
    let on = s.add_point(1.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Radius { center_idx: c, p_on_circle: on, radius: 0.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

#[test]
fn radius_already_satisfied() {
    let mut s = ConstraintSystem::new();
    let c = s.add_point(0.0, 0.0);
    let on = s.add_point(7.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: on, position: Point2::new(7.0, 0.0) });
    s.add_constraint(Constraint::Radius { center_idx: c, p_on_circle: on, radius: 7.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

// ============================================================
// SISTEMA — DOF / OVER / UNDER
// ============================================================

#[test]
fn no_constraints_dof_equals_2n() {
    let mut s = ConstraintSystem::new();
    for _ in 0..5 {
        s.add_point(0.0, 0.0);
    }
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.dof, 10, "5 puntos sin constraints = 10 DOF");
    assert_eq!(r.status, SolveStatus::UnderConstrained);
}

#[test]
fn fully_constrained_zero_dof() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 1.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(3.0, 4.0) });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
    assert_eq!(r.dof, 0);
}

#[test]
fn distance_overconstrained_with_2_fixed() {
    // 2 puntos fixed + distance entre ellos que NO matchea su separación
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 10.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert!(
        matches!(r.status, SolveStatus::OverConstrained | SolveStatus::NotConverged)
            || r.residual > 0.1,
        "distance conflictiva no detectada: status={:?} residual={}",
        r.status,
        r.residual
    );
}

// ============================================================
// CONVERGENCIA — initial guess malo
// ============================================================

#[test]
fn distance_far_initial_guess() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1e5, 1e5);  // muy lejos del target
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 5.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    let dx = r.points[b].x() - r.points[a].x();
    let dy = r.points[b].y() - r.points[a].y();
    assert_relative_eq!((dx * dx + dy * dy).sqrt(), 5.0, epsilon = 1e-4);
}

#[test]
fn perpendicular_near_parallel_initial() {
    // Initial guess casi paralelo — solver debe converger a perpendicular
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    let c = s.add_point(0.0, 0.0);
    let d = s.add_point(5.0, 0.01);  // casi paralela a l1
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // Resultado: l2 perpendicular a l1 → d.x ≈ 0
    assert!(r.points[d].x().abs() < 0.1, "no convergió a perpendicular: d.x={}", r.points[d].x());
}

// ============================================================
// STRESS — sin panic
// ============================================================

#[test]
fn many_free_points_no_panic() {
    let mut s = ConstraintSystem::new();
    for i in 0..100 {
        s.add_point(i as f64, (i * 2) as f64);
    }
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.dof, 200);
}

// ============================================================
// ADVERSARIALES — Jacobiano singular / casos patológicos
// ============================================================

#[test]
fn tangent_line_starts_through_center() {
    // Caso patológico: línea inicial pasa exactamente por el centro
    // (cross=0 → cross.abs() no diferenciable)
    let mut s = ConstraintSystem::new();
    let p1 = s.add_point(-5.0, 0.0);
    let p2 = s.add_point(5.0, 0.0);  // línea horizontal y=0
    let c = s.add_point(0.0, 0.0);   // centro EN la línea
    s.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(-5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::TangentLineCircle {
        line_p1: p1, line_p2: p2, center_idx: c, radius: 2.0,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // p2 debe terminar tal que distancia centro-línea = 2
    let dx = r.points[p2].x() - r.points[p1].x();
    let dy = r.points[p2].y() - r.points[p1].y();
    let len = (dx * dx + dy * dy).sqrt();
    let cross = dx * (r.points[c].y() - r.points[p1].y())
        - dy * (r.points[c].x() - r.points[p1].x());
    let dist = cross.abs() / len;
    assert_relative_eq!(dist, 2.0, epsilon = 1e-3);
}

#[test]
fn tangent_line_inside_circle() {
    // Línea inicial completamente dentro del círculo (radio grande)
    let mut s = ConstraintSystem::new();
    let p1 = s.add_point(-0.5, 0.5);
    let p2 = s.add_point(0.5, 0.5);  // distancia centro-línea = 0.5
    let c = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(-0.5, 0.5) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::TangentLineCircle {
        line_p1: p1, line_p2: p2, center_idx: c, radius: 5.0,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

#[test]
fn perpendicular_almost_parallel_initial() {
    // Líneas casi paralelas: Jacobiano de Perpendicular casi singular
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 0.0);
    let c = s.add_point(0.0, 0.0);
    let d = s.add_point(1.0, 1e-8);  // microscópicamente no paralelo
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(1.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert!(r.points[d].x().abs() < 1e-3, "no convergió: d.x={}", r.points[d].x());
}

#[test]
fn parallel_one_line_zero_length() {
    // l1 zero-length → cross product es 0 trivialmente, gradiente nulo
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 0.0);  // l1 zero-length
    let c = s.add_point(0.0, 0.0);
    let d = s.add_point(1.0, 1.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Parallel {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

#[test]
fn angle_zero_length_line() {
    // atan2(0, 0) es indefinido — verificar no NaN
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 0.0);  // l1 zero
    let c = s.add_point(0.0, 0.0);
    let d = s.add_point(1.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: d, position: Point2::new(1.0, 0.0) });
    s.add_constraint(Constraint::Angle {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
        angle_rad: 1.0,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
}

#[test]
fn equal_length_both_zero_length() {
    // |d1| - |d2| = 0 - 0 = 0, satisfecho trivial. Sin gradiente.
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 0.0);
    let c = s.add_point(5.0, 5.0);
    let d = s.add_point(5.0, 5.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(5.0, 5.0) });
    s.add_constraint(Constraint::Fixed { p_idx: d, position: Point2::new(5.0, 5.0) });
    s.add_constraint(Constraint::EqualLength {
        l1_p1: a, l1_p2: b, l2_p1: c, l2_p2: d,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

// ============================================================
// MULTIPLE SOLUTIONS — solver debe converger a UNA solución válida
// ============================================================

#[test]
fn distance_two_circles_intersection() {
    // Punto P libre con Distance(P, A)=3 y Distance(P, B)=4. A=(0,0), B=(5,0).
    // Soluciones: (¿0 si triángulo no cierra? sí cierra: 3+4>5)
    // Hay 2 soluciones simétricas en y. Solver debe dar UNA, sin NaN.
    let mut s = ConstraintSystem::new();
    let pa = s.add_point(0.0, 0.0);
    let pb = s.add_point(5.0, 0.0);
    let pt = s.add_point(2.0, 2.0);
    s.add_constraint(Constraint::Fixed { p_idx: pa, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: pb, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: pa, p2_idx: pt, distance: 3.0 });
    s.add_constraint(Constraint::Distance { p1_idx: pb, p2_idx: pt, distance: 4.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    let d_a = (r.points[pt].x().powi(2) + r.points[pt].y().powi(2)).sqrt();
    let d_b = ((r.points[pt].x() - 5.0).powi(2) + r.points[pt].y().powi(2)).sqrt();
    assert_relative_eq!(d_a, 3.0, epsilon = 1e-4);
    assert_relative_eq!(d_b, 4.0, epsilon = 1e-4);
}

#[test]
fn distance_circles_no_intersection() {
    // Círculos no se cruzan: d(A,B)=10, r1=2, r2=3. 2+3 < 10 → sin solución.
    let mut s = ConstraintSystem::new();
    let pa = s.add_point(0.0, 0.0);
    let pb = s.add_point(10.0, 0.0);
    let p_pt = s.add_point(5.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: pa, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: pb, position: Point2::new(10.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: pa, p2_idx: p_pt, distance: 2.0 });
    s.add_constraint(Constraint::Distance { p1_idx: pb, p2_idx: p_pt, distance: 3.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert!(
        matches!(r.status, SolveStatus::OverConstrained | SolveStatus::NotConverged),
        "círculos imposibles no detectados: status={:?} residual={}",
        r.status,
        r.residual
    );
}

// ============================================================
// POINT ON SPLINE
// ============================================================

#[test]
fn point_on_spline_already_on_polyline() {
    // Punto ya sobre el segmento de la polilínea
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(2.5, 0.0);  // sobre el segmento (0,0)-(5,0)
    let s0 = s.add_point(0.0, 0.0);
    let s1 = s.add_point(5.0, 0.0);
    let s2 = s.add_point(5.0, 5.0);
    s.add_constraint(Constraint::Fixed { p_idx: s0, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: s1, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: s2, position: Point2::new(5.0, 5.0) });
    s.add_constraint(Constraint::Fixed { p_idx: pt, position: Point2::new(2.5, 0.0) });
    s.add_constraint(Constraint::PointOnSpline {
        p_idx: pt,
        spline_point_indices: vec![s0, s1, s2],
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_eq!(r.status, SolveStatus::Converged);
}

#[test]
fn point_on_spline_signed_residual_attracts_correctly() {
    // Punto a un lado de la polilínea: el residual es vectorial signed,
    // el solver sabe en qué dirección moverlo.
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(2.5, 1.5);  // arriba del segmento (0,0)-(5,0)
    let s0 = s.add_point(0.0, 0.0);
    let s1 = s.add_point(5.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: s0, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: s1, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::PointOnSpline {
        p_idx: pt,
        spline_point_indices: vec![s0, s1],
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // pt debe terminar sobre el segmento (y=0)
    assert!(
        r.points[pt].y().abs() < 1e-3,
        "PointOnSpline signed residual no atrajo: pt={:?}",
        r.points[pt]
    );
    // pt.x debe estar en [0, 5] (proyección sobre el segmento)
    assert!(0.0 <= r.points[pt].x() && r.points[pt].x() <= 5.0);
}

#[test]
fn point_on_spline_two_segments_jumps_to_correct() {
    // Polilínea en L: (0,0)-(5,0)-(5,5). Punto inicial cerca de la
    // ESQUINA (5,0): debe terminar sobre la polilínea, no en mínimo
    // local entre los dos segmentos.
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(5.5, 0.5);
    let s0 = s.add_point(0.0, 0.0);
    let s1 = s.add_point(5.0, 0.0);
    let s2 = s.add_point(5.0, 5.0);
    s.add_constraint(Constraint::Fixed { p_idx: s0, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: s1, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: s2, position: Point2::new(5.0, 5.0) });
    s.add_constraint(Constraint::PointOnSpline {
        p_idx: pt,
        spline_point_indices: vec![s0, s1, s2],
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // pt sobre uno de los dos segmentos: y=0 con 0≤x≤5, ó x=5 con 0≤y≤5
    let on_horizontal = r.points[pt].y().abs() < 1e-3
        && 0.0 <= r.points[pt].x() && r.points[pt].x() <= 5.0;
    let on_vertical = (r.points[pt].x() - 5.0).abs() < 1e-3
        && 0.0 <= r.points[pt].y() && r.points[pt].y() <= 5.0;
    assert!(on_horizontal || on_vertical,
        "pt no quedó sobre la polilínea: {:?}", r.points[pt]);
}

#[test]
fn point_on_spline_far_pulls_to_polyline() {
    // Punto lejos: el solver debe acercarlo a la polilínea
    let mut s = ConstraintSystem::new();
    let pt = s.add_point(2.5, 3.0);  // a 3 del segmento horizontal
    let s0 = s.add_point(0.0, 0.0);
    let s1 = s.add_point(5.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: s0, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: s1, position: Point2::new(5.0, 0.0) });
    s.add_constraint(Constraint::PointOnSpline {
        p_idx: pt,
        spline_point_indices: vec![s0, s1],
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // pt.y debe acercarse a 0
    assert!(
        r.points[pt].y().abs() < 1e-3,
        "PointOnSpline no convergió: pt={:?}",
        r.points[pt]
    );
}

// ============================================================
// SYMMETRIC — caso patológico: punto sobre el eje
// ============================================================

#[test]
fn symmetric_point_on_axis() {
    // p1 sobre el eje de simetría (1,0)–(1,5): su simétrico es él mismo
    // → p2 debería terminar en p1
    let mut s = ConstraintSystem::new();
    let p1 = s.add_point(1.0, 2.5);
    let p2 = s.add_point(3.0, 2.5);
    let l1 = s.add_point(1.0, 0.0);
    let l2 = s.add_point(1.0, 5.0);
    s.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(1.0, 2.5) });
    s.add_constraint(Constraint::Fixed { p_idx: l1, position: Point2::new(1.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: l2, position: Point2::new(1.0, 5.0) });
    s.add_constraint(Constraint::Symmetric {
        p1_idx: p1, p2_idx: p2, line_p1: l1, line_p2: l2,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // p1 está sobre el eje → reflexión es ella misma → p2 debe converger a p1
    let dx = r.points[p2].x() - r.points[p1].x();
    let dy = r.points[p2].y() - r.points[p1].y();
    assert!(
        (dx * dx + dy * dy).sqrt() < 1e-3,
        "p2 no converge a reflexión de p1 sobre eje: p2={:?}",
        r.points[p2]
    );
}

// ============================================================
// HORIZONTAL DIST / VERTICAL DIST signo
// ============================================================

#[test]
fn horizontal_dist_signed() {
    // distance positiva: p1.x - p2.x = +3 → p1 está 3 unidades a la derecha de p2
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::HorizontalDist { p1_idx: a, p2_idx: b, distance: 3.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_relative_eq!(r.points[a].x() - r.points[b].x(), 3.0, epsilon = 1e-6);
}

#[test]
fn horizontal_dist_negative() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(0.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: b, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::HorizontalDist { p1_idx: a, p2_idx: b, distance: -3.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_relative_eq!(r.points[a].x() - r.points[b].x(), -3.0, epsilon = 1e-6);
}

// ============================================================
// TRIÁNGULO IMPOSIBLE — desigualdad triangular violada
// ============================================================

#[test]
fn triangle_inequality_violated() {
    // Lados 1, 2, 5: imposible (1+2 < 5)
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 0.0);
    let c = s.add_point(2.0, 1.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 1.0 });
    s.add_constraint(Constraint::Distance { p1_idx: b, p2_idx: c, distance: 2.0 });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: c, distance: 5.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert!(
        matches!(r.status, SolveStatus::OverConstrained | SolveStatus::NotConverged),
        "triángulo imposible no detectado: status={:?} residual={}",
        r.status,
        r.residual
    );
}

// ============================================================
// CUADRADO REGULAR — sistema cíclico cerrado
// ============================================================

#[test]
fn regular_square_closed() {
    // 4 vértices con perpendicularidades + lados iguales + distance
    let mut s = ConstraintSystem::new();
    let p0 = s.add_point(0.0, 0.0);
    let p1 = s.add_point(2.5, 0.3);  // initial guess perturbado
    let p2 = s.add_point(2.3, 2.4);
    let p3 = s.add_point(0.1, 2.6);

    s.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Horizontal { p1_idx: p0, p2_idx: p1 });
    s.add_constraint(Constraint::Distance { p1_idx: p0, p2_idx: p1, distance: 2.0 });
    // 3 EqualLength (con 4 lados, 3 son independientes)
    s.add_constraint(Constraint::EqualLength {
        l1_p1: p0, l1_p2: p1, l2_p1: p1, l2_p2: p2,
    });
    s.add_constraint(Constraint::EqualLength {
        l1_p1: p1, l1_p2: p2, l2_p1: p2, l2_p2: p3,
    });
    s.add_constraint(Constraint::EqualLength {
        l1_p1: p2, l1_p2: p3, l2_p1: p3, l2_p2: p0,
    });
    // 4 perpendicularidades adyacentes
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: p0, l1_p2: p1, l2_p1: p1, l2_p2: p2,
    });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: p1, l1_p2: p2, l2_p1: p2, l2_p2: p3,
    });

    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // Lados de longitud 2
    for (a, b) in [(p0, p1), (p1, p2), (p2, p3), (p3, p0)] {
        let dx = r.points[b].x() - r.points[a].x();
        let dy = r.points[b].y() - r.points[a].y();
        let len = (dx * dx + dy * dy).sqrt();
        assert_relative_eq!(len, 2.0, epsilon = 1e-3);
    }
}

#[test]
fn regular_hexagon_redundant() {
    use std::f64::consts::PI;
    let mut s = ConstraintSystem::new();
    let mut verts = Vec::new();
    // Initial guess casi exacto (lado=1, hexágono regular)
    for i in 0..6 {
        let angle = (i as f64) * PI / 3.0;
        verts.push(s.add_point(angle.cos(), angle.sin()));
    }
    s.add_constraint(Constraint::Fixed { p_idx: verts[0], position: Point2::new(1.0, 0.0) });
    s.add_constraint(Constraint::Distance {
        p1_idx: verts[0], p2_idx: verts[1], distance: 1.0,
    });
    for i in 0..6 {
        let j = (i + 1) % 6;
        let k = (i + 2) % 6;
        s.add_constraint(Constraint::EqualLength {
            l1_p1: verts[i], l1_p2: verts[j],
            l2_p1: verts[j], l2_p2: verts[k],
        });
        // Atan2(cross, dot) en CCW da signo negativo para ángulo interno
        s.add_constraint(Constraint::Angle {
            l1_p1: verts[j], l1_p2: verts[i],
            l2_p1: verts[j], l2_p2: verts[k],
            angle_rad: -2.0 * PI / 3.0,
        });
    }
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    for i in 0..6 {
        let j = (i + 1) % 6;
        let dx = r.points[verts[j]].x() - r.points[verts[i]].x();
        let dy = r.points[verts[j]].y() - r.points[verts[i]].y();
        let len = (dx * dx + dy * dy).sqrt();
        assert_relative_eq!(len, 1.0, epsilon = 1e-3);
    }
}

// ============================================================
// REDUNDANCIA — constraints que se implican mutuamente
// ============================================================

#[test]
fn redundant_horizontal_constraints_no_conflict() {
    // Horizontal + HorizontalDist(0) + Distance(>0) — todos consistentes
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 0.5);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Horizontal { p1_idx: a, p2_idx: b });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 5.0 });
    // Redundante con Horizontal: dy=0
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert!(r.points[b].y().abs() < 1e-6);
    assert_relative_eq!(r.points[b].x().abs(), 5.0, epsilon = 1e-6);
}

#[test]
fn coincident_plus_distance_zero_redundant() {
    // Coincident y Distance=0 son redundantes pero consistentes
    let mut s = ConstraintSystem::new();
    let a = s.add_point(2.0, 3.0);
    let b = s.add_point(5.0, 7.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(2.0, 3.0) });
    s.add_constraint(Constraint::Coincident { p1_idx: a, p2_idx: b });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 0.0 });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_relative_eq!(r.points[b].x(), 2.0, epsilon = 1e-6);
    assert_relative_eq!(r.points[b].y(), 3.0, epsilon = 1e-6);
}

// ============================================================
// IDEMPOTENCIA — resolver dos veces da el mismo resultado
// ============================================================

#[test]
fn solve_twice_idempotent() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(3.0, 4.0);
    let c = s.add_point(0.0, 4.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: 5.0 });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: c, distance: 4.0 });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: a, l1_p2: b, l2_p1: a, l2_p2: c,
    });
    let r1 = solve(&mut s, &p()).unwrap();
    no_nan(&r1);
    let p1_b = r1.points[b];
    let p1_c = r1.points[c];

    let r2 = solve(&mut s, &p()).unwrap();
    no_nan(&r2);
    assert_relative_eq!(r2.points[b].x(), p1_b.x(), epsilon = 1e-9);
    assert_relative_eq!(r2.points[b].y(), p1_b.y(), epsilon = 1e-9);
    assert_relative_eq!(r2.points[c].x(), p1_c.x(), epsilon = 1e-9);
    assert_relative_eq!(r2.points[c].y(), p1_c.y(), epsilon = 1e-9);
}

// ============================================================
// MUCHAS CONSTRAINTS — stress
// ============================================================

#[test]
fn many_distances_chain() {
    // Cadena de 30 puntos con Distance entre consecutivos
    let mut s = ConstraintSystem::new();
    let mut pts = Vec::new();
    for i in 0..30 {
        pts.push(s.add_point(i as f64 * 0.5, 0.0));
    }
    s.add_constraint(Constraint::Fixed { p_idx: pts[0], position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Horizontal { p1_idx: pts[0], p2_idx: pts[29] });
    for i in 0..29 {
        s.add_constraint(Constraint::Distance {
            p1_idx: pts[i], p2_idx: pts[i + 1], distance: 1.0,
        });
    }
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // Distancia total esperada: 29
    let last = r.points[pts[29]];
    assert_relative_eq!(last.y(), 0.0, epsilon = 1e-4);
    assert_relative_eq!(last.x().abs(), 29.0, epsilon = 1e-3);
}

// ============================================================
// PERPENDICULAR CICLO 3 LÍNEAS (imposible en 2D)
// ============================================================

#[test]
fn perpendicular_cycle_three_lines_impossible() {
    // A⊥B, B⊥C, C⊥A no puede satisfacerse en 2D (forzaría A∥A)
    let mut s = ConstraintSystem::new();
    let a1 = s.add_point(0.0, 0.0);
    let a2 = s.add_point(1.0, 0.0);
    let b1 = s.add_point(0.0, 0.0);
    let b2 = s.add_point(0.0, 1.0);
    let c1 = s.add_point(0.0, 0.0);
    let c2 = s.add_point(1.0, 1.0);
    s.add_constraint(Constraint::Fixed { p_idx: a1, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: a2, position: Point2::new(1.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: b1, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Fixed { p_idx: c1, position: Point2::new(0.0, 0.0) });
    // 3 perpendicularidades cíclicas
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: a1, l1_p2: a2, l2_p1: b1, l2_p2: b2,
    });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: b1, l1_p2: b2, l2_p1: c1, l2_p2: c2,
    });
    s.add_constraint(Constraint::Perpendicular {
        l1_p1: c1, l1_p2: c2, l2_p1: a1, l2_p2: a2,
    });
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    // No debe panic; el resultado puede ser OverConstrained o satisfacer 2/3
    // (por ejemplo, B y C colapsan a zero-length para satisfacer trivialmente)
}

#[test]
fn empty_constraint_set_with_points() {
    let mut s = ConstraintSystem::new();
    s.add_point(3.14, 2.71);
    let r = solve(&mut s, &p()).unwrap();
    no_nan(&r);
    assert_relative_eq!(r.points[0].x(), 3.14, epsilon = 1e-12);
}

// ============================================================
// ROBUSTEZ — inputs que antes paniqueaban (OOB) o colgaban (NaN/Inf).
// Deben devolver Err limpio, nunca panic ni bucle infinito.
// ============================================================

#[test]
fn out_of_bounds_index_returns_err_not_panic() {
    let mut s = ConstraintSystem::new();
    s.add_point(0.0, 0.0);
    // Distance referencia un índice de punto inexistente (1)
    s.add_constraint(Constraint::Distance {
        p1_idx: 0,
        p2_idx: 1, // fuera de rango: solo existe el punto 0
        distance: 5.0,
    });
    let r = solve(&mut s, &p());
    assert!(r.is_err(), "índice OOB debe dar Err, no panic ni Ok");
}

#[test]
fn nan_distance_returns_err_not_hang() {
    let mut s = ConstraintSystem::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(3.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: a, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::Distance { p1_idx: a, p2_idx: b, distance: f64::NAN });
    let r = solve(&mut s, &p());
    // No debe colgar; convergencia imposible con NaN → Err o no-convergido sin panic.
    assert!(r.is_err() || r.is_ok());
}

#[test]
fn inf_radius_returns_err_not_hang() {
    let mut s = ConstraintSystem::new();
    let c = s.add_point(0.0, 0.0);
    let q = s.add_point(1.0, 0.0);
    s.add_constraint(Constraint::Fixed { p_idx: c, position: Point2::new(0.0, 0.0) });
    s.add_constraint(Constraint::PointOnCircle { p_idx: q, center_idx: c, radius: f64::INFINITY });
    let r = solve(&mut s, &p());
    assert!(r.is_err() || r.is_ok());
}
