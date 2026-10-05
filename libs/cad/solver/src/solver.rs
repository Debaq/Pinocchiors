use nalgebra::{DMatrix, DVector};
use nalgebra_sparse::CscMatrix;

use crate::error::{Result, SolverError};
use crate::system::ConstraintSystem;
use crate::types::{SolveResult, SolveStatus};

/// Materializar CscMatrix sparse a DMatrix denso (para Cholesky/SVD nalgebra).
/// Solo recorre los non-zeros — O(nnz), no O(n²).
fn cscmatrix_to_dense(m: &CscMatrix<f64>) -> DMatrix<f64> {
    let mut dense = DMatrix::<f64>::zeros(m.nrows(), m.ncols());
    for (r, c, v) in m.triplet_iter() {
        dense[(r, c)] = *v;
    }
    dense
}

/// Parámetros del solver
#[derive(Debug, Clone)]
pub struct SolverParams {
    pub max_iterations: usize,
    pub tolerance: f64,
    pub lm_initial_lambda: f64,
    pub lm_lambda_factor: f64,
    pub line_search_alpha: f64,
    pub line_search_beta: f64,
}

impl Default for SolverParams {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-10,
            lm_initial_lambda: 1e-3,
            lm_lambda_factor: 10.0,
            line_search_alpha: 1e-4,
            line_search_beta: 0.5,
        }
    }
}

/// Resolver mientras se arrastra un punto: lo fija temporalmente en `target`
/// y resuelve con tolerancia relajada (velocidad antes que precisión).
/// La restricción temporal se quita siempre, converja o no.
pub fn solve_drag(
    system: &mut ConstraintSystem,
    dragged_point: usize,
    target: crate::types::Point2,
) -> Result<SolveResult> {
    if dragged_point >= system.points.len() {
        return Err(SolverError::InvalidConstraint(format!(
            "punto arrastrado {dragged_point} no existe"
        )));
    }
    system.points[dragged_point] = target;
    system.add_constraint(crate::constraint::Constraint::Fixed {
        p_idx: dragged_point,
        position: target,
    });
    let params = SolverParams {
        max_iterations: 30,
        tolerance: 1e-6,
        ..SolverParams::default()
    };
    let result = solve(system, &params);
    system.constraints.pop();
    // Si el destino choca con las restricciones (el punto no puede llegar ahí),
    // LM deja un compromiso que no cumple ninguna: se vuelve a resolver sin el
    // punto fijo desde esa posición, que queda lo más cerca posible del mouse.
    match result {
        Ok(r) if r.status == SolveStatus::OverConstrained || r.residual > 1e-6 => {
            solve(system, &params)
        }
        other => other,
    }
}

/// Resolver sistema de constraints usando Newton-Raphson con fallback a Levenberg-Marquardt.
/// Descompone en sub-sistemas independientes si es posible.
pub fn solve(system: &mut ConstraintSystem, params: &SolverParams) -> Result<SolveResult> {
    // Validar índices de punto ANTES de evaluar: un índice fuera de rango
    // venido de Python haría panic (`points[idx]`) en residuals/jacobian y
    // crashearía el .so (y Blender). Reportarlo como error limpio.
    let n_pts = system.points.len();
    for c in &system.constraints {
        for idx in c.point_indices() {
            if idx >= n_pts {
                return Err(SolverError::InvalidConstraint(format!(
                    "índice de punto {idx} fuera de rango ({n_pts} puntos)"
                )));
            }
        }
    }

    let n_eq = system.num_equations();
    let n_var = system.num_variables();

    if n_eq == 0 {
        return Ok(SolveResult {
            status: SolveStatus::UnderConstrained,
            dof: n_var as i32,
            iterations: 0,
            residual: 0.0,
            points: system.points.clone(),
        });
    }

    // Descomponer en sub-sistemas independientes
    let subsystems = system.decompose();

    if subsystems.len() > 1 {
        return solve_decomposed(system, subsystems, params);
    }

    // Sistema único — resolver directo
    solve_single(system, params)
}

/// Resolver sub-sistemas independientes y recombinar resultados.
fn solve_decomposed(
    system: &mut ConstraintSystem,
    mut subsystems: Vec<ConstraintSystem>,
    params: &SolverParams,
) -> Result<SolveResult> {
    let mut total_dof = 0i32;
    let mut total_iterations = 0;
    let mut max_residual = 0.0f64;
    let mut worst_status = SolveStatus::Converged;

    // Construir mapeo inverso: para cada sub-sistema, qué puntos globales tiene
    // Reconstruir desde el decompose — los puntos están en orden sorted
    let n_points = system.points.len();
    let mut visited = vec![false; n_points];
    let mut subsystem_point_maps: Vec<Vec<usize>> = Vec::new();

    for sub in &subsystems {
        let mut global_indices = Vec::new();
        // Encontrar qué puntos globales corresponden a este sub-sistema
        // buscando por coordenadas (decompose los crea en orden sorted)
        for sub_pt in &sub.points {
            for gi in 0..n_points {
                if !visited[gi]
                    && (system.points[gi].x() - sub_pt.x()).abs() < 1e-15
                    && (system.points[gi].y() - sub_pt.y()).abs() < 1e-15
                {
                    global_indices.push(gi);
                    visited[gi] = true;
                    break;
                }
            }
        }
        subsystem_point_maps.push(global_indices);
    }

    // Resolver cada sub-sistema
    for (i, sub) in subsystems.iter_mut().enumerate() {
        let result = solve_single(sub, params)?;

        total_dof += result.dof;
        total_iterations = total_iterations.max(result.iterations);
        max_residual = max_residual.max(result.residual);

        if result.status == SolveStatus::OverConstrained {
            worst_status = SolveStatus::OverConstrained;
        } else if result.status == SolveStatus::NotConverged
            && worst_status != SolveStatus::OverConstrained
        {
            worst_status = SolveStatus::NotConverged;
        } else if result.status == SolveStatus::UnderConstrained
            && worst_status == SolveStatus::Converged
        {
            worst_status = SolveStatus::UnderConstrained;
        }

        // Copiar resultados de vuelta al sistema global
        if let Some(map) = subsystem_point_maps.get(i) {
            for (local, &global) in map.iter().enumerate() {
                if local < result.points.len() && global < system.points.len() {
                    system.points[global] = result.points[local];
                }
            }
        }
    }

    Ok(SolveResult {
        status: worst_status,
        dof: total_dof,
        iterations: total_iterations,
        residual: max_residual,
        points: system.points.clone(),
    })
}

/// Resolver un sistema único (sin descomposición).
fn solve_single(system: &mut ConstraintSystem, params: &SolverParams) -> Result<SolveResult> {
    // Intentar Newton-Raphson primero
    match newton_raphson(system, params) {
        Ok(result) if result.status == SolveStatus::Converged => return Ok(result),
        Ok(result) if result.status == SolveStatus::UnderConstrained => return Ok(result),
        _ => {}
    }

    // Fallback: Levenberg-Marquardt
    levenberg_marquardt(system, params)
}

/// Newton-Raphson sparse con backtracking line search.
/// Usa Jacobiano CSR → JᵀJ sparse → Cholesky sparse (CscCholesky).
/// Fallback a SVD denso si la factorización sparse falla.
fn newton_raphson(system: &mut ConstraintSystem, params: &SolverParams) -> Result<SolveResult> {
    let n_var = system.num_variables();

    for iter in 0..params.max_iterations {
        let res = system.residuals();
        let res_vec = DVector::from_vec(res.clone());
        let residual_norm = res_vec.norm();

        // Guard NaN/Inf: con un residual no-finito (target NaN/Inf, geometría
        // degenerada, overflow) la comparación `< tolerance` es falsa para SIEMPRE
        // y el SVD posterior puede colgar el hilo → Blender congelado. Cortar.
        if !residual_norm.is_finite() {
            return Err(SolverError::NotConverged {
                max_iter: iter,
                residual: residual_norm,
            });
        }

        if residual_norm < params.tolerance {
            let dof = compute_dof(system);
            let status = if dof == 0 {
                SolveStatus::Converged
            } else if dof > 0 {
                SolveStatus::UnderConstrained
            } else {
                SolveStatus::OverConstrained
            };
            return Ok(SolveResult {
                status,
                dof,
                iterations: iter,
                residual: residual_norm,
                points: system.points.clone(),
            });
        }

        // Jacobiano sparse (CSR) → CSC para multiplicaciones eficientes
        let jac_csr = system.jacobian_sparse();
        let jac_csc = CscMatrix::from(&jac_csr);
        let jt_csc = jac_csc.transpose();
        let neg_res = -&res_vec;

        // JᵀJ y Jᵀ(-r) sparse — recorren solo nnz, no n² entries
        let jtj_sparse: CscMatrix<f64> = &jt_csc * &jac_csc;
        let jtr: DVector<f64> = &jt_csc * &neg_res;

        // Materializar JᵀJ a denso + regularización Tikhonov εI para Cholesky denso
        // (nalgebra-sparse Cholesky es experimental — la dense es robusta)
        let mut jtj_reg = cscmatrix_to_dense(&jtj_sparse);
        let eps = 1e-12;
        for i in 0..n_var {
            jtj_reg[(i, i)] += eps;
        }

        let dx = match jtj_reg.cholesky() {
            Some(chol) => chol.solve(&jtr),
            None => {
                // Fallback SVD si Cholesky falla (jacobian dense para SVD)
                let jac_dense = system.jacobian_dense();
                let svd = jac_dense.svd(true, true);
                match svd.solve(&neg_res, params.tolerance) {
                    Ok(dx) => dx,
                    Err(_) => return Err(SolverError::SingularJacobian(iter)),
                }
            }
        };

        // Backtracking line search — J·dx vía sparse-dense
        let mut alpha = 1.0;
        let state_orig = system.state_vector();
        let jdx: DVector<f64> = &jac_csc * &dx;
        let directional_derivative = res_vec.dot(&jdx);

        for _ in 0..20 {
            let new_state: Vec<f64> = state_orig
                .iter()
                .zip(dx.iter())
                .map(|(s, d)| s + alpha * d)
                .collect();
            system.apply_state(&new_state);

            let new_res = system.residuals();
            let new_norm = DVector::from_vec(new_res).norm();

            if new_norm <= residual_norm + params.line_search_alpha * alpha * directional_derivative
            {
                break;
            }

            alpha *= params.line_search_beta;
        }
    }

    let res = system.residuals();
    let residual = DVector::from_vec(res).norm();
    Err(SolverError::NotConverged {
        max_iter: params.max_iterations,
        residual,
    })
}

/// Tras LM sin alcanzar tolerance: ¿es over-constrained inconsistente o no-convergencia?
/// Si ||J^T r|| ≈ 0, estamos en un mínimo local de ||F||² → solución de mínimos cuadrados.
/// Si además residual > tolerance, el sistema no admite solución exacta → OverConstrained.
fn classify_lm_termination(
    system: &ConstraintSystem,
    residual: f64,
    params: &SolverParams,
    iterations: usize,
) -> Option<SolveResult> {
    let jac = system.jacobian_dense();
    let res_vec = DVector::from_vec(system.residuals());
    let grad = jac.transpose() * &res_vec;
    let grad_norm = grad.norm();

    // Tolerancia del gradiente: relajada respecto a tolerance del residual.
    // grad ≈ 0 indica que estamos en el mínimo del problema de mínimos cuadrados.
    let grad_tol = (params.tolerance.sqrt()).max(1e-6);
    if grad_norm > grad_tol {
        return None; // No es mínimo — es no-convergencia genuina
    }

    // Estamos en mínimo de LSQ. Determinar status por rank del Jacobiano.
    let n_var = system.num_variables();
    let svd = jac.svd(false, false);
    let s_max = svd
        .singular_values
        .iter()
        .copied()
        .fold(0f64, f64::max);
    let threshold = 1e-8 * s_max.max(1.0);
    let rank = svd
        .singular_values
        .iter()
        .filter(|&&s| s > threshold)
        .count();
    let dof = n_var as i32 - rank as i32;

    let status = if residual > params.tolerance.sqrt() {
        SolveStatus::OverConstrained
    } else if dof > 0 {
        SolveStatus::UnderConstrained
    } else {
        SolveStatus::Converged
    };

    Some(SolveResult {
        status,
        dof,
        iterations,
        residual,
        points: system.points.clone(),
    })
}

/// Levenberg-Marquardt: minimizar ||F(x)||² con regularización
fn levenberg_marquardt(
    system: &mut ConstraintSystem,
    params: &SolverParams,
) -> Result<SolveResult> {
    let n_var = system.num_variables();
    let mut lambda = params.lm_initial_lambda;

    for iter in 0..params.max_iterations {
        let res = system.residuals();
        let res_vec = DVector::from_vec(res);
        let residual_norm = res_vec.norm();

        // Guard NaN/Inf (ver newton_raphson): evita bucle/cuelgue con residual no-finito.
        if !residual_norm.is_finite() {
            return Err(SolverError::NotConverged {
                max_iter: iter,
                residual: residual_norm,
            });
        }

        if residual_norm < params.tolerance {
            let dof = compute_dof(system);
            let status = if dof == 0 {
                SolveStatus::Converged
            } else if dof > 0 {
                SolveStatus::UnderConstrained
            } else {
                SolveStatus::OverConstrained
            };
            return Ok(SolveResult {
                status,
                dof,
                iterations: iter,
                residual: residual_norm,
                points: system.points.clone(),
            });
        }

        // Jacobiano sparse → JᵀJ sparse → Cholesky denso
        let jac_csr = system.jacobian_sparse();
        let jac_csc = CscMatrix::from(&jac_csr);
        let jt_csc = jac_csc.transpose();
        let jtj_sparse: CscMatrix<f64> = &jt_csc * &jac_csc;
        let jtr: DVector<f64> = &jt_csc * &res_vec;

        // (JᵀJ + λI) dx = -Jᵀr
        let mut lhs = cscmatrix_to_dense(&jtj_sparse);
        for i in 0..n_var {
            lhs[(i, i)] += lambda;
        }
        let rhs = -&jtr;

        let decomp = lhs.lu();
        let dx = match decomp.solve(&rhs) {
            Some(dx) => dx,
            None => {
                lambda *= params.lm_lambda_factor;
                continue;
            }
        };

        // Evaluar si el paso mejora
        let state_orig = system.state_vector();
        let new_state: Vec<f64> = state_orig
            .iter()
            .zip(dx.iter())
            .map(|(s, d)| s + d)
            .collect();
        system.apply_state(&new_state);

        let new_res = system.residuals();
        let new_norm = DVector::from_vec(new_res).norm();

        if new_norm < residual_norm {
            // Paso aceptado, reducir lambda
            lambda /= params.lm_lambda_factor;
        } else {
            // Paso rechazado, restaurar y aumentar lambda
            system.apply_state(&state_orig);
            lambda *= params.lm_lambda_factor;
        }
    }

    let res = system.residuals();
    let residual = DVector::from_vec(res).norm();

    // ¿Es mínimo de LSQ (over-constrained) o no-convergencia real?
    if let Some(result) = classify_lm_termination(system, residual, params, params.max_iterations) {
        return Ok(result);
    }

    Err(SolverError::NotConverged {
        max_iter: params.max_iterations,
        residual,
    })
}

/// Calcular grados de libertad: num_variables - rank(J)
fn compute_dof(system: &ConstraintSystem) -> i32 {
    let n_var = system.num_variables();
    let n_eq = system.num_equations();

    if n_eq == 0 {
        return n_var as i32;
    }

    let jac = system.jacobian_dense();
    let svd = jac.svd(false, false);

    // Contar valores singulares no-despreciables
    let threshold = 1e-8 * svd.singular_values[0];
    let rank = svd
        .singular_values
        .iter()
        .filter(|&&s| s > threshold)
        .count();

    n_var as i32 - rank as i32
}
