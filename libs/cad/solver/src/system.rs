use nalgebra::DMatrix;
use nalgebra_sparse::CsrMatrix;

use crate::constraint::Constraint;
use crate::types::Point2;

/// Sistema de restricciones: colección de puntos + constraints
#[derive(Debug, Clone)]
pub struct ConstraintSystem {
    pub points: Vec<Point2>,
    pub constraints: Vec<Constraint>,
}

impl ConstraintSystem {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            constraints: Vec::new(),
        }
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> usize {
        let idx = self.points.len();
        self.points.push(Point2::new(x, y));
        idx
    }

    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Número total de variables (2 por punto: x, y)
    pub fn num_variables(&self) -> usize {
        self.points.len() * 2
    }

    /// Número total de ecuaciones
    pub fn num_equations(&self) -> usize {
        self.constraints.iter().map(|c| c.num_equations()).sum()
    }

    /// Evaluar todos los residuales F(x)
    pub fn residuals(&self) -> Vec<f64> {
        let mut res = Vec::with_capacity(self.num_equations());
        for constraint in &self.constraints {
            res.extend(constraint.residuals(&self.points));
        }
        res
    }

    /// Construir Jacobiano sparse (CSR)
    pub fn jacobian_sparse(&self) -> CsrMatrix<f64> {
        let n_eq = self.num_equations();
        let n_var = self.num_variables();

        // Recolectar triplets (row, col, value)
        let mut rows = Vec::new();
        let mut cols = Vec::new();
        let mut vals = Vec::new();

        let mut row_offset = 0;
        for constraint in &self.constraints {
            let entries = constraint.jacobian_entries(&self.points);
            for (local_row, point_idx, dfdx, dfdy) in entries {
                let global_row = row_offset + local_row;
                let col_x = point_idx * 2;
                let col_y = point_idx * 2 + 1;

                if dfdx.abs() > 1e-15 {
                    rows.push(global_row);
                    cols.push(col_x);
                    vals.push(dfdx);
                }
                if dfdy.abs() > 1e-15 {
                    rows.push(global_row);
                    cols.push(col_y);
                    vals.push(dfdy);
                }
            }
            row_offset += constraint.num_equations();
        }

        // Construir COO → CSR
        let coo = nalgebra_sparse::CooMatrix::try_from_triplets(
            n_eq, n_var, rows, cols, vals,
        )
        .unwrap_or_else(|_| nalgebra_sparse::CooMatrix::new(n_eq, n_var));

        CsrMatrix::from(&coo)
    }

    /// Construir Jacobiano denso (mantener para compatibilidad con solver)
    pub fn jacobian(&self) -> Vec<Vec<f64>> {
        let n_eq = self.num_equations();
        let n_var = self.num_variables();
        let mut jac = vec![vec![0.0; n_var]; n_eq];

        let mut row_offset = 0;
        for constraint in &self.constraints {
            let entries = constraint.jacobian_entries(&self.points);
            for (local_row, point_idx, dfdx, dfdy) in entries {
                let global_row = row_offset + local_row;
                let col_x = point_idx * 2;
                let col_y = point_idx * 2 + 1;
                jac[global_row][col_x] += dfdx;
                jac[global_row][col_y] += dfdy;
            }
            row_offset += constraint.num_equations();
        }

        jac
    }

    /// Construir Jacobiano como DMatrix (para el solver que usa nalgebra)
    pub fn jacobian_dense(&self) -> DMatrix<f64> {
        let n_eq = self.num_equations();
        let n_var = self.num_variables();
        let jac_data = self.jacobian();
        DMatrix::from_fn(n_eq, n_var, |i, j| jac_data[i][j])
    }

    /// Extraer vector de estado plano [x0, y0, x1, y1, ...]
    pub fn state_vector(&self) -> Vec<f64> {
        let mut state = Vec::with_capacity(self.num_variables());
        for p in &self.points {
            state.push(p.x());
            state.push(p.y());
        }
        state
    }

    /// Aplicar vector de estado a los puntos
    pub fn apply_state(&mut self, state: &[f64]) {
        for (i, p) in self.points.iter_mut().enumerate() {
            p.co.x = state[i * 2];
            p.co.y = state[i * 2 + 1];
        }
    }

    /// Sparsity ratio del Jacobiano (para diagnóstico)
    pub fn sparsity_ratio(&self) -> f64 {
        let sparse = self.jacobian_sparse();
        let total = sparse.nrows() * sparse.ncols();
        if total == 0 {
            return 0.0;
        }
        1.0 - (sparse.nnz() as f64 / total as f64)
    }

    /// Descomponer en sub-sistemas independientes via grafo de dependencias.
    ///
    /// Si punto A no comparte ningún constraint con punto B, se resuelven
    /// en sistemas separados — reduce O(n³) a O(k³) donde k < n.
    pub fn decompose(&self) -> Vec<ConstraintSystem> {
        use std::collections::{HashMap, HashSet, VecDeque};

        let n_points = self.points.len();
        if n_points == 0 || self.constraints.is_empty() {
            return vec![self.clone()];
        }

        // Construir grafo: para cada punto, qué constraints lo afectan
        let mut point_constraints: Vec<Vec<usize>> = vec![vec![]; n_points];
        for (ci, constraint) in self.constraints.iter().enumerate() {
            for &pi in &constraint.point_indices() {
                if pi < n_points {
                    point_constraints[pi].push(ci);
                }
            }
        }

        // BFS para encontrar componentes conexos
        let mut visited = vec![false; n_points];
        let mut components: Vec<HashSet<usize>> = Vec::new(); // sets de point indices

        for start in 0..n_points {
            if visited[start] {
                continue;
            }

            let mut component = HashSet::new();
            let mut queue = VecDeque::new();
            queue.push_back(start);
            visited[start] = true;

            while let Some(current) = queue.pop_front() {
                component.insert(current);

                // Encontrar todos los puntos conectados via constraints compartidos
                for &ci in &point_constraints[current] {
                    for &neighbor in &self.constraints[ci].point_indices() {
                        if neighbor < n_points && !visited[neighbor] {
                            visited[neighbor] = true;
                            queue.push_back(neighbor);
                        }
                    }
                }
            }

            components.push(component);
        }

        // Si solo hay un componente, no vale la pena descomponer
        if components.len() <= 1 {
            return vec![self.clone()];
        }

        // Construir sub-sistemas
        let mut subsystems = Vec::with_capacity(components.len());

        for component in &components {
            let mut sub = ConstraintSystem::new();

            // Mapeo de índices globales a locales
            let sorted_points: Vec<usize> = {
                let mut v: Vec<usize> = component.iter().copied().collect();
                v.sort();
                v
            };
            let mut global_to_local: HashMap<usize, usize> = HashMap::new();
            for (local, &global) in sorted_points.iter().enumerate() {
                global_to_local.insert(global, local);
                sub.points.push(self.points[global]);
            }

            // Agregar constraints que pertenecen a este componente
            for constraint in &self.constraints {
                let indices = constraint.point_indices();
                if indices.iter().all(|pi| component.contains(pi)) {
                    // Remapear índices
                    let remapped = constraint.remap_indices(&global_to_local);
                    sub.constraints.push(remapped);
                }
            }

            subsystems.push(sub);
        }

        subsystems
    }
}

impl Default for ConstraintSystem {
    fn default() -> Self {
        Self::new()
    }
}
