//! Diagnóstico de constraints — identificar cuáles conflictúan o son redundantes.

use crate::system::ConstraintSystem;

/// Resultado de diagnóstico del sistema de constraints.
#[derive(Debug, Clone)]
pub struct DiagnosticResult {
    /// Índices de constraints que conflictúan (residual no-cero post-solve)
    pub conflicting: Vec<usize>,
    /// Índices de constraints redundantes (no aportan rank al Jacobiano)
    pub redundant: Vec<usize>,
    /// Subset mínimo de constraints cuya remoción haría el sistema solvable.
    /// Vacío si el sistema ya es consistente.
    pub minimal_conflict_set: Vec<usize>,
    /// Índices de puntos con DOF libre (sub-restringido)
    pub free_points: Vec<usize>,
    /// DOF por punto: (punto_idx, dof_count)
    pub dof_per_point: Vec<(usize, i32)>,
    /// Resumen general
    pub status: DiagnosticStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticStatus {
    /// Sistema bien definido (DOF=0, residual≈0)
    WellConstrained,
    /// Faltan constraints (DOF>0)
    UnderConstrained,
    /// Constraints redundantes pero consistentes (DOF=0, sobran ecuaciones, residual≈0)
    OverConstrainedConsistent,
    /// Constraints en conflicto irreconciliable (residual>0)
    OverConstrainedConflicting,
}

/// Analizar sistema y diagnosticar problemas.
///
/// Pasos:
/// 1. Resolver el sistema (con un clon, no muta el original)
/// 2. Calcular residual por constraint para detectar conflictos reales
/// 3. Análisis de rank del Jacobiano para detectar redundancia
/// 4. Si hay conflicto, encontrar subset mínimo
pub fn diagnose(system: &ConstraintSystem) -> DiagnosticResult {
    use crate::solver::{solve, SolverParams};

    let n_var = system.num_variables();
    let n_eq = system.num_equations();
    let n_points = system.points.len();

    if n_eq == 0 || n_var == 0 {
        return DiagnosticResult {
            conflicting: vec![],
            redundant: vec![],
            minimal_conflict_set: vec![],
            free_points: (0..n_points).collect(),
            dof_per_point: (0..n_points).map(|i| (i, 2)).collect(),
            status: DiagnosticStatus::UnderConstrained,
        };
    }

    let full_jac = system.jacobian_dense();
    let full_rank = compute_rank(&full_jac);
    let dof = n_var as i32 - full_rank as i32;

    // Resolver clon para obtener residuales por constraint en el óptimo
    let mut sys_solved = system.clone();
    let _ = solve(&mut sys_solved, &SolverParams::default());
    let per_constraint_residuals = compute_per_constraint_residuals(&sys_solved);
    let total_residual: f64 = per_constraint_residuals.iter().map(|r| r * r).sum::<f64>().sqrt();

    // Tolerancia: residual relativo al tamaño del sistema
    let conflict_threshold = 1e-4;
    let is_inconsistent = total_residual > conflict_threshold;

    // Redundantes: constraints que no aportan rank
    let mut redundant = Vec::new();
    let mut eq_offset = 0;
    for (ci, constraint) in system.constraints.iter().enumerate() {
        let n_eqs_this = constraint.num_equations();
        let reduced_rank = rank_without_rows(&full_jac, eq_offset, n_eqs_this);
        if reduced_rank == full_rank {
            redundant.push(ci);
        }
        eq_offset += n_eqs_this;
    }

    // Conflictivos: residual por constraint alto (solo si sistema inconsistente)
    let conflicting: Vec<usize> = if is_inconsistent {
        per_constraint_residuals
            .iter()
            .enumerate()
            .filter(|(_, r)| r.abs() > conflict_threshold)
            .map(|(i, _)| i)
            .collect()
    } else {
        vec![]
    };

    // Minimal conflict set: greedy
    let minimal_conflict_set = if is_inconsistent {
        find_minimal_conflict_set(system, &conflicting)
    } else {
        vec![]
    };

    // DOF por punto: contar columnas independientes que el punto contribuye
    let mut dof_per_point = Vec::with_capacity(n_points);
    for pi in 0..n_points {
        let dof_pt = compute_point_dof(&full_jac, pi);
        dof_per_point.push((pi, dof_pt));
    }
    let free_points: Vec<usize> = dof_per_point
        .iter()
        .filter(|(_, d)| *d > 0)
        .map(|(pi, _)| *pi)
        .collect();

    let status = if is_inconsistent {
        DiagnosticStatus::OverConstrainedConflicting
    } else if !redundant.is_empty() && dof == 0 {
        DiagnosticStatus::OverConstrainedConsistent
    } else if dof > 0 {
        DiagnosticStatus::UnderConstrained
    } else {
        DiagnosticStatus::WellConstrained
    };

    DiagnosticResult {
        conflicting,
        redundant,
        minimal_conflict_set,
        free_points,
        dof_per_point,
        status,
    }
}

/// Norma del residual de cada constraint (suma cuadrática de sus filas).
fn compute_per_constraint_residuals(system: &ConstraintSystem) -> Vec<f64> {
    let res = system.residuals();
    let mut out = Vec::with_capacity(system.constraints.len());
    let mut offset = 0;
    for c in &system.constraints {
        let n = c.num_equations();
        let norm = res[offset..offset + n].iter().map(|r| r * r).sum::<f64>().sqrt();
        out.push(norm);
        offset += n;
    }
    out
}

/// Greedy: tirar constraints conflictivos uno por uno (en orden de mayor residual)
/// hasta que el residual total caiga bajo tolerancia. Devuelve los tirados.
fn find_minimal_conflict_set(system: &ConstraintSystem, candidates: &[usize]) -> Vec<usize> {
    use crate::solver::{solve, SolverParams};

    if candidates.is_empty() {
        return vec![];
    }

    // Ordenar candidatos por residual descendente (los peores primero)
    let mut sys_test = system.clone();
    let _ = solve(&mut sys_test, &SolverParams::default());
    let res = compute_per_constraint_residuals(&sys_test);
    let mut ordered: Vec<usize> = candidates.to_vec();
    ordered.sort_by(|&a, &b| res[b].partial_cmp(&res[a]).unwrap_or(std::cmp::Ordering::Equal));

    let conflict_threshold = 1e-4;
    let mut removed: Vec<usize> = Vec::new();

    for &idx in &ordered {
        // Construir sistema sin las constraints removidas + idx
        let mut sub = system.clone();
        let mut all_remove = removed.clone();
        all_remove.push(idx);
        all_remove.sort_unstable();
        // Eliminar de mayor a menor para no invalidar índices
        for &r_idx in all_remove.iter().rev() {
            if r_idx < sub.constraints.len() {
                sub.constraints.remove(r_idx);
            }
        }
        let _ = solve(&mut sub, &SolverParams::default());
        let res_total: f64 = sub
            .residuals()
            .iter()
            .map(|r| r * r)
            .sum::<f64>()
            .sqrt();
        removed.push(idx);
        if res_total < conflict_threshold {
            break;
        }
    }

    removed
}

/// Rank del Jacobiano sin las filas [start_row, start_row + n_rows)
fn rank_without_rows(jac: &nalgebra::DMatrix<f64>, start_row: usize, n_rows: usize) -> usize {
    use nalgebra::DMatrix;
    let total_rows = jac.nrows();
    let cols = jac.ncols();
    if n_rows >= total_rows {
        return 0;
    }
    let mut reduced = DMatrix::zeros(total_rows - n_rows, cols);
    let mut dst = 0;
    for src in 0..total_rows {
        if src >= start_row && src < start_row + n_rows {
            continue;
        }
        for c in 0..cols {
            reduced[(dst, c)] = jac[(src, c)];
        }
        dst += 1;
    }
    compute_rank(&reduced)
}

/// DOF de un punto: 2 - rank de las 2 columnas del Jacobiano correspondientes.
fn compute_point_dof(jac: &nalgebra::DMatrix<f64>, point_idx: usize) -> i32 {
    let cols = jac.ncols();
    let cx = point_idx * 2;
    let cy = point_idx * 2 + 1;
    if cy >= cols {
        return 0;
    }
    // Submatriz con sólo las 2 columnas del punto
    use nalgebra::DMatrix;
    let mut sub = DMatrix::zeros(jac.nrows(), 2);
    for r in 0..jac.nrows() {
        sub[(r, 0)] = jac[(r, cx)];
        sub[(r, 1)] = jac[(r, cy)];
    }
    let r = compute_rank(&sub);
    (2 - r as i32).max(0)
}

fn compute_rank(mat: &nalgebra::DMatrix<f64>) -> usize {
    if mat.nrows() == 0 || mat.ncols() == 0 {
        return 0;
    }
    let svd = mat.clone().svd(false, false);
    let threshold = 1e-8 * svd.singular_values[0];
    svd.singular_values
        .iter()
        .filter(|&&s| s > threshold)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Constraint;
    use crate::types::Point2;

    #[test]
    fn test_diagnose_overconstrained() {
        let mut sys = ConstraintSystem::new();
        let p0 = sys.add_point(0.0, 0.0);
        let p1 = sys.add_point(5.0, 0.0);

        // Fijar p0
        sys.add_constraint(Constraint::Fixed {
            p_idx: p0,
            position: Point2::new(0.0, 0.0),
        });
        // Fijar p1
        sys.add_constraint(Constraint::Fixed {
            p_idx: p1,
            position: Point2::new(5.0, 0.0),
        });
        // Distancia (redundante — ya están fijados)
        sys.add_constraint(Constraint::Distance {
            p1_idx: p0,
            p2_idx: p1,
            distance: 5.0,
        });

        let diag = diagnose(&sys);
        // Distance es redundante
        assert!(!diag.redundant.is_empty(), "Debería detectar constraint redundante");
        println!("Redundantes: {:?}", diag.redundant);
        println!("Conflictos: {:?}", diag.conflicting);
    }

    #[test]
    fn test_diagnose_free_points() {
        let mut sys = ConstraintSystem::new();
        let p0 = sys.add_point(0.0, 0.0);
        let _p1 = sys.add_point(5.0, 3.0);

        sys.add_constraint(Constraint::Fixed {
            p_idx: p0,
            position: Point2::new(0.0, 0.0),
        });

        let diag = diagnose(&sys);
        assert!(diag.free_points.contains(&1), "p1 debería ser free");
        assert_eq!(diag.status, DiagnosticStatus::UnderConstrained);
    }

    #[test]
    fn test_diagnose_overconstrained_consistent() {
        // 2 Fixed redundantes pero CONSISTENTES + Distance que matchea
        let mut sys = ConstraintSystem::new();
        let p0 = sys.add_point(0.0, 0.0);
        let p1 = sys.add_point(5.0, 0.0);
        sys.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(0.0, 0.0) });
        sys.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(5.0, 0.0) });
        sys.add_constraint(Constraint::Distance { p1_idx: p0, p2_idx: p1, distance: 5.0 });
        let diag = diagnose(&sys);
        assert_eq!(diag.status, DiagnosticStatus::OverConstrainedConsistent);
        assert!(!diag.redundant.is_empty());
        assert!(diag.conflicting.is_empty(),
            "consistente: NO debería haber conflicting. Got: {:?}", diag.conflicting);
        assert!(diag.minimal_conflict_set.is_empty());
    }

    #[test]
    fn test_diagnose_overconstrained_conflicting() {
        // 2 Fixed con posiciones distintas: imposible
        let mut sys = ConstraintSystem::new();
        let p0 = sys.add_point(0.0, 0.0);
        sys.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(1.0, 1.0) });
        sys.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(2.0, 2.0) });
        let diag = diagnose(&sys);
        assert_eq!(diag.status, DiagnosticStatus::OverConstrainedConflicting);
        assert!(!diag.conflicting.is_empty(), "debería detectar conflicto");
        assert!(!diag.minimal_conflict_set.is_empty(),
            "minimal_conflict_set debería contener al menos uno");
        // Sacando UNO de los Fixed debería ser suficiente
        assert!(diag.minimal_conflict_set.len() <= 2);
    }

    #[test]
    fn test_diagnose_distance_conflict_with_two_fixed() {
        // p0=(0,0) fixed, p1=(5,0) fixed, Distance=10 imposible
        let mut sys = ConstraintSystem::new();
        let p0 = sys.add_point(0.0, 0.0);
        let p1 = sys.add_point(5.0, 0.0);
        sys.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(0.0, 0.0) });
        sys.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(5.0, 0.0) });
        sys.add_constraint(Constraint::Distance { p1_idx: p0, p2_idx: p1, distance: 10.0 });
        let diag = diagnose(&sys);
        assert_eq!(diag.status, DiagnosticStatus::OverConstrainedConflicting);
        // El conflicto debe involucrar al Distance (índice 2)
        assert!(diag.conflicting.contains(&2),
            "Distance es el constraint que rompe: {:?}", diag.conflicting);
    }

    #[test]
    fn test_diagnose_well_constrained() {
        let mut sys = ConstraintSystem::new();
        let p0 = sys.add_point(0.0, 0.0);
        let p1 = sys.add_point(5.0, 0.0);
        sys.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(0.0, 0.0) });
        sys.add_constraint(Constraint::Fixed { p_idx: p1, position: Point2::new(5.0, 0.0) });
        let diag = diagnose(&sys);
        assert_eq!(diag.status, DiagnosticStatus::WellConstrained);
        assert!(diag.conflicting.is_empty());
        assert!(diag.redundant.is_empty());
    }

    #[test]
    fn test_diagnose_dof_per_point() {
        let mut sys = ConstraintSystem::new();
        let p0 = sys.add_point(0.0, 0.0);
        let _p1 = sys.add_point(5.0, 5.0);
        sys.add_constraint(Constraint::Fixed { p_idx: p0, position: Point2::new(0.0, 0.0) });
        // p1 libre: 2 DOF
        // p0 fijo: 0 DOF
        let diag = diagnose(&sys);
        let dof_p0 = diag.dof_per_point.iter().find(|(i, _)| *i == 0).unwrap().1;
        let dof_p1 = diag.dof_per_point.iter().find(|(i, _)| *i == 1).unwrap().1;
        assert_eq!(dof_p0, 0);
        assert_eq!(dof_p1, 2);
    }
}
