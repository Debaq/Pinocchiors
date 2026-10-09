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

    // Un solo SVD da el rango y las constraints que no aportan rank
    let mut blocks = Vec::with_capacity(system.constraints.len());
    let mut eq_offset = 0;
    for constraint in &system.constraints {
        blocks.push((eq_offset, constraint.num_equations()));
        eq_offset += constraint.num_equations();
    }
    let (full_rank, redundant_flags) = rank_and_redundant(&full_jac, &blocks);
    let dof = n_var as i32 - full_rank as i32;
    let redundant: Vec<usize> = redundant_flags.into_iter().enumerate().filter(|&(_, r)| r).map(|(ci, _)| ci).collect();

    // Tolerancia: residual relativo al tamaño del sistema
    let conflict_threshold = 1e-4;

    // Residuales por constraint en el óptimo. Lo normal es diagnosticar lo
    // recién resuelto: si ya cumple, resolver de nuevo no cambia nada
    let mut per_constraint_residuals = compute_per_constraint_residuals(system);
    if per_constraint_residuals.iter().map(|r| r * r).sum::<f64>().sqrt() > conflict_threshold {
        let mut sys_solved = system.clone();
        let _ = solve(&mut sys_solved, &SolverParams::default());
        per_constraint_residuals = compute_per_constraint_residuals(&sys_solved);
    }
    let total_residual: f64 = per_constraint_residuals.iter().map(|r| r * r).sum::<f64>().sqrt();
    let is_inconsistent = total_residual > conflict_threshold;

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

    // DOF por punto: cuánto del espacio nulo del Jacobiano mueve al punto
    // (los movimientos que las restricciones permiten). Mirar solo las
    // columnas del punto marcaba como fijo a cualquiera tocado por dos
    // restricciones, aunque pudiera deslizarse.
    let null = nullspace(&full_jac);
    let mut dof_per_point = Vec::with_capacity(n_points);
    for pi in 0..n_points {
        dof_per_point.push((pi, point_dof_in_nullspace(&null, pi)));
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
/// Rango del jacobiano y, para cada bloque de filas (inicio, cantidad), si
/// quitarlo deja el rango igual. Con un solo SVD: las filas R sobran si las columnas de U con valor
/// singular no nulo no las cubren enteras, es decir si I − U_R·U_Rᵀ tiene
/// rango completo (sus filas están en el espacio de las demás). Antes era un
/// SVD por restricción: con cientos (un patrón de relleno) tardaba segundos.
fn rank_and_redundant(jac: &nalgebra::DMatrix<f64>, blocks: &[(usize, usize)]) -> (usize, Vec<bool>) {
    use nalgebra::DMatrix;
    if jac.nrows() == 0 || jac.ncols() == 0 {
        return (0, vec![true; blocks.len()]);
    }
    let svd = jac.clone().svd(true, false);
    let u = svd.u.as_ref().expect("SVD con U");
    let max = svd.singular_values.iter().fold(0.0f64, |m, &v| m.max(v));
    // Mismo umbral que `compute_rank`
    let cols: Vec<usize> = (0..svd.singular_values.len()).filter(|&i| svd.singular_values[i] > 1e-8 * max).collect();
    let redundant = blocks
        .iter()
        .map(|&(start, n)| {
            if n == 0 {
                return true;
            }
            let m = DMatrix::from_fn(n, n, |i, j| {
                let dot: f64 = cols.iter().map(|&c| u[(start + i, c)] * u[(start + j, c)]).sum();
                if i == j { 1.0 - dot } else { -dot }
            });
            let eig = nalgebra::SymmetricEigen::new(m);
            eig.eigenvalues.iter().all(|&v| v > 1e-9)
        })
        .collect();
    (cols.len(), redundant)
}

#[cfg(test)]
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
/// Base del espacio nulo (columnas) por autovectores de JᵀJ.
fn nullspace(jac: &nalgebra::DMatrix<f64>) -> nalgebra::DMatrix<f64> {
    let n = jac.ncols();
    let jtj = jac.transpose() * jac;
    let eig = nalgebra::SymmetricEigen::new(jtj);
    let max = eig.eigenvalues.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300);
    let keep: Vec<usize> = (0..n).filter(|&i| eig.eigenvalues[i].abs() <= 1e-10 * max.max(1.0)).collect();
    nalgebra::DMatrix::from_fn(n, keep.len(), |r, c| eig.eigenvectors[(r, keep[c])])
}

fn point_dof_in_nullspace(null: &nalgebra::DMatrix<f64>, point_idx: usize) -> i32 {
    let (cx, cy) = (point_idx * 2, point_idx * 2 + 1);
    if cy >= null.nrows() || null.ncols() == 0 {
        return 0;
    }
    let sub = nalgebra::DMatrix::from_fn(2, null.ncols(), |r, c| null[(if r == 0 { cx } else { cy }, c)]);
    let svd = sub.svd(false, false);
    svd.singular_values.iter().filter(|&&v| v > 1e-6).count() as i32
}

#[allow(dead_code)]
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

    /// Los redundantes con un solo SVD coinciden con quitar fila por fila
    #[test]
    fn test_redundant_blocks_matches_rank_without_rows() {
        let mut systems = Vec::new();
        // Rectángulo con una vertical de más, una cota repetida y un punto fijo dos veces
        let mut sys = ConstraintSystem::new();
        let p: Vec<usize> = [(0.0, 0.0), (4.0, 0.1), (4.2, 3.0), (0.1, 3.1)].iter().map(|&(x, y)| sys.add_point(x, y)).collect();
        sys.add_constraint(Constraint::Fixed { p_idx: p[0], position: Point2::new(0.0, 0.0) });
        sys.add_constraint(Constraint::Horizontal { p1_idx: p[0], p2_idx: p[1] });
        sys.add_constraint(Constraint::Horizontal { p1_idx: p[3], p2_idx: p[2] });
        sys.add_constraint(Constraint::Vertical { p1_idx: p[1], p2_idx: p[2] });
        sys.add_constraint(Constraint::Vertical { p1_idx: p[0], p2_idx: p[3] });
        sys.add_constraint(Constraint::Parallel { l1_p1: p[0], l1_p2: p[3], l2_p1: p[1], l2_p2: p[2] });
        sys.add_constraint(Constraint::Distance { p1_idx: p[0], p2_idx: p[1], distance: 4.0 });
        sys.add_constraint(Constraint::Distance { p1_idx: p[0], p2_idx: p[1], distance: 4.0 });
        sys.add_constraint(Constraint::Fixed { p_idx: p[0], position: Point2::new(0.0, 0.0) });
        systems.push(sys);
        // Patrón: mismo desplazamiento encadenado y una cota que lo repite
        let mut sys = ConstraintSystem::new();
        let p: Vec<usize> = (0..4).map(|i| sys.add_point(3.0 * i as f64, 0.5 * i as f64)).collect();
        sys.add_constraint(Constraint::EqualVector { a1: p[0], a2: p[1], b1: p[1], b2: p[2] });
        sys.add_constraint(Constraint::EqualVector { a1: p[0], a2: p[1], b1: p[2], b2: p[3] });
        sys.add_constraint(Constraint::EqualVector { a1: p[1], a2: p[2], b1: p[2], b2: p[3] });
        sys.add_constraint(Constraint::HorizontalDist { p1_idx: p[0], p2_idx: p[1], distance: 3.0 });
        systems.push(sys);
        for sys in systems {
            let jac = sys.jacobian_dense();
            let full = compute_rank(&jac);
            let mut blocks = Vec::new();
            let mut off = 0;
            for c in &sys.constraints {
                blocks.push((off, c.num_equations()));
                off += c.num_equations();
            }
            let (rank, fast) = rank_and_redundant(&jac, &blocks);
            assert_eq!(rank, full);
            let slow: Vec<bool> = blocks.iter().map(|&(s, n)| rank_without_rows(&jac, s, n) == full).collect();
            assert_eq!(fast, slow);
            assert!(fast.iter().any(|&r| r), "el caso tiene redundantes");
        }
    }
}
