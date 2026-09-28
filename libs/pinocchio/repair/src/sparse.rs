//! Mínimos cuadrados dispersos con solución directa.
//!
//! Ecuaciones normales AᵀA x = Aᵀb, reordenadas con Cuthill–McKee inverso y
//! factorizadas con Cholesky en envolvente (skyline). En mallas (grafos casi
//! planos) el ancho de banda tras el reordenamiento crece como √n, así que
//! factorizar cuesta O(n²) y es mucho más rápido y fiable que un método
//! iterativo sobre un sistema bi-laplaciano mal condicionado.

use pinocchio_math::{Real, Vector3};
use std::collections::VecDeque;

/// Resuelve min Σ‖(A x)ᵣ − bᵣ‖² para `columns` incógnitas vectoriales.
///
/// Cada fila de A es una lista `(columna, coeficiente)`. Devuelve `None` si
/// el sistema es singular (alguna incógnita sin determinar).
pub(crate) fn solve_least_squares(
    rows: &[Vec<(usize, Real)>],
    rhs: &[Vector3],
    columns: usize,
) -> Option<Vec<Vector3>> {
    if columns == 0 {
        return Some(Vec::new());
    }

    // AᵀA como listas por columna y Aᵀb
    let mut entries: Vec<Vec<(usize, Real)>> = vec![Vec::new(); columns];
    let mut atb = vec![Vector3::zero(); columns];
    for (row, &b) in rows.iter().zip(rhs) {
        for &(i, wi) in row {
            atb[i] += b * wi;
            for &(j, wj) in row {
                entries[i].push((j, wi * wj));
            }
        }
    }
    for list in &mut entries {
        list.sort_unstable_by_key(|&(j, _)| j);
        list.dedup_by(|next, kept| {
            if next.0 == kept.0 {
                kept.1 += next.1;
                true
            } else {
                false
            }
        });
    }

    // Orden Cuthill–McKee inverso: new_of[vieja] = nueva
    let order = reverse_cuthill_mckee(&entries);
    let mut new_of = vec![0; columns];
    for (new, &old) in order.iter().enumerate() {
        new_of[old] = new;
    }

    // Envolvente: cada fila guarda desde su primera columna no nula hasta la
    // diagonal
    let first: Vec<usize> = order
        .iter()
        .enumerate()
        .map(|(r, &old)| entries[old].iter().map(|&(j, _)| new_of[j]).min().unwrap_or(r).min(r))
        .collect();
    let mut offset = vec![0usize; columns + 1];
    for r in 0..columns {
        offset[r + 1] = offset[r] + (r - first[r] + 1);
    }
    let mut l = vec![0.0; offset[columns]];
    let max_diag = (0..columns)
        .flat_map(|i| entries[i].iter().filter(move |e| e.0 == i).map(|e| e.1))
        .fold(0.0, Real::max);
    for (r, &old) in order.iter().enumerate() {
        for &(j, w) in &entries[old] {
            let c = new_of[j];
            if c <= r {
                l[offset[r] + c - first[r]] += w;
            }
        }
        // Regularización mínima contra ceros numéricos
        l[offset[r] + r - first[r]] += 1e-14 * max_diag;
    }

    // Cholesky en el lugar: L Lᵀ = M
    for r in 0..columns {
        for c in first[r]..=r {
            let start = first[r].max(first[c]);
            let mut sum = l[offset[r] + c - first[r]];
            for k in start..c {
                sum -= l[offset[r] + k - first[r]] * l[offset[c] + k - first[c]];
            }
            if c < r {
                l[offset[r] + c - first[r]] = sum / l[offset[c] + c - first[c]];
            } else {
                if sum.is_nan() || sum <= 0.0 {
                    return None;
                }
                l[offset[r] + r - first[r]] = sum.sqrt();
            }
        }
    }

    // L y = b ; Lᵀ x = y
    let mut x: Vec<Vector3> = order.iter().map(|&old| atb[old]).collect();
    for r in 0..columns {
        let mut sum = x[r];
        for k in first[r]..r {
            sum -= x[k] * l[offset[r] + k - first[r]];
        }
        x[r] = sum / l[offset[r] + r - first[r]];
    }
    for r in (0..columns).rev() {
        x[r] /= l[offset[r] + r - first[r]];
        let xr = x[r];
        for k in first[r]..r {
            x[k] -= xr * l[offset[r] + k - first[r]];
        }
    }

    let mut solution = vec![Vector3::zero(); columns];
    for (new, &old) in order.iter().enumerate() {
        solution[old] = x[new];
    }
    Some(solution)
}

/// Orden Cuthill–McKee inverso de un grafo dado por listas de adyacencia
/// (puede incluir la diagonal). Recorre cada componente en anchura desde un
/// nodo de grado mínimo, visitando vecinos de menor a mayor grado.
fn reverse_cuthill_mckee(adjacency: &[Vec<(usize, Real)>]) -> Vec<usize> {
    let n = adjacency.len();
    let degree: Vec<usize> = adjacency.iter().map(|a| a.len()).collect();
    let mut by_degree: Vec<usize> = (0..n).collect();
    by_degree.sort_by_key(|&v| degree[v]);

    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut queue = VecDeque::new();
    let mut neighbours = Vec::new();
    for &seed in &by_degree {
        if visited[seed] {
            continue;
        }
        visited[seed] = true;
        queue.push_back(seed);
        while let Some(v) = queue.pop_front() {
            order.push(v);
            neighbours.clear();
            neighbours.extend(adjacency[v].iter().map(|&(w, _)| w).filter(|&w| !visited[w]));
            neighbours.sort_by_key(|&w| degree[w]);
            for &w in &neighbours {
                if !visited[w] {
                    visited[w] = true;
                    queue.push_back(w);
                }
            }
        }
    }
    order.reverse();
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_overdetermined_system() {
        // x0 = 1, x1 = 2, x0 + x1 = 3 (consistente) con x vectorial
        let rows = vec![vec![(0, 1.0)], vec![(1, 1.0)], vec![(0, 1.0), (1, 1.0)]];
        let b = [Vector3::new(1., 0., 0.), Vector3::new(2., 1., 0.), Vector3::new(3., 1., 0.)];
        let x = solve_least_squares(&rows, &b, 2).unwrap();
        assert!((x[0] - Vector3::new(1., 0., 0.)).length() < 1e-9);
        assert!((x[1] - Vector3::new(2., 1., 0.)).length() < 1e-9);
    }

    #[test]
    fn least_squares_average() {
        // x = 1 y x = 3: el óptimo es 2
        let rows = vec![vec![(0, 1.0)], vec![(0, 1.0)]];
        let b = [Vector3::new(1., 1., 1.), Vector3::new(3., 3., 3.)];
        let x = solve_least_squares(&rows, &b, 1).unwrap();
        assert!((x[0] - Vector3::new(2., 2., 2.)).length() < 1e-9);
    }

    #[test]
    fn laplacian_chain_matches_linear_interpolation() {
        // Cadena fija en los extremos (0 y 10): x_i − (x_{i−1} + x_{i+1})/2 = 0
        let n = 9;
        let mut rows = Vec::new();
        let mut b = Vec::new();
        for i in 0..n {
            let mut row = vec![(i, 1.0)];
            let mut rhs = Vector3::zero();
            if i > 0 {
                row.push((i - 1, -0.5));
            }
            if i + 1 < n {
                row.push((i + 1, -0.5));
            } else {
                rhs += Vector3::new(10.0, 0.0, 0.0) * 0.5;
            }
            rows.push(row);
            b.push(rhs);
        }
        let x = solve_least_squares(&rows, &b, n).unwrap();
        for (i, p) in x.iter().enumerate() {
            assert!((p.x() - (i + 1) as Real).abs() < 1e-7, "{i}: {p:?}");
        }
    }
}
