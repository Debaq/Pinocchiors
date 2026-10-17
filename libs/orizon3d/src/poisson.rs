//! Reconstrucción de Poisson de la nube de puntos con normales.
//!
//! Las normales orientadas (hacia la cámara que vio cada punto) forman un campo
//! de vectores `V` que apunta hacia adentro del objeto. Se busca la función
//! indicadora suave `χ` tal que `∇χ ≈ V`, es decir `Δχ = ∇·V`, en una rejilla
//! regular (Laplaciano de 7 puntos, gradiente conjugado, `χ = 0` en el borde).
//! La superficie es la isosuperficie de `χ` al valor medio que toma en los puntos,
//! extraída con Surface Nets. A diferencia del campo MLS de [`crate::mesh`], el
//! resultado es siempre cerrado y liso: tapa los huecos solo (de ahí que se
//! recorte lo que queda lejos de los datos).

use std::collections::HashMap;

use crate::mesh::{laplacian_smooth, orient_normals, push_quad, Mesh};
use crate::pointcloud::PointCloud;

/// Tope de nodos de la rejilla (memoria y tiempo del solver)
const MAX_NODES: usize = 12_000_000;
/// Margen de nodos vacíos alrededor de la nube
const PAD: usize = 3;

struct Grid {
    nx: usize,
    ny: usize,
    nz: usize,
}

impl Grid {
    fn idx(&self, i: usize, j: usize, k: usize) -> usize {
        (k * self.ny + j) * self.nx + i
    }
    fn len(&self) -> usize {
        self.nx * self.ny * self.nz
    }
}

/// Aplica `y = -Δx` (Laplaciano de 7 puntos con `x = 0` fuera de la rejilla)
fn neg_laplacian(g: &Grid, x: &[f32], y: &mut [f32]) {
    for k in 0..g.nz {
        for j in 0..g.ny {
            for i in 0..g.nx {
                let id = g.idx(i, j, k);
                let c = x[id];
                let mut s = 6.0 * c;
                s -= if i > 0 { x[id - 1] } else { 0.0 };
                s -= if i + 1 < g.nx { x[id + 1] } else { 0.0 };
                s -= if j > 0 { x[id - g.nx] } else { 0.0 };
                s -= if j + 1 < g.ny { x[id + g.nx] } else { 0.0 };
                s -= if k > 0 { x[id - g.nx * g.ny] } else { 0.0 };
                s -= if k + 1 < g.nz { x[id + g.nx * g.ny] } else { 0.0 };
                y[id] = s;
            }
        }
    }
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum()
}

/// Gradiente conjugado para `-Δx = b`
fn solve(g: &Grid, b: &[f32], iterations: usize) -> Vec<f32> {
    let n = g.len();
    let mut x = vec![0f32; n];
    let mut r = b.to_vec();
    let mut p = r.clone();
    let mut ap = vec![0f32; n];
    let mut rs = dot(&r, &r);
    let stop = rs * 1e-8;
    for _ in 0..iterations {
        if rs <= stop {
            break;
        }
        neg_laplacian(g, &p, &mut ap);
        let denom = dot(&p, &ap);
        if denom <= 0.0 {
            break;
        }
        let alpha = (rs / denom) as f32;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        let rs_new = dot(&r, &r);
        let beta = (rs_new / rs) as f32;
        for i in 0..n {
            p[i] = r[i] + beta * p[i];
        }
        rs = rs_new;
    }
    x
}

/// Dilatación por caja de radio `r` nodos (tres pasadas, una por eje)
fn dilate(g: &Grid, occupied: &mut [bool], r: usize) {
    let mut tmp = vec![false; occupied.len()];
    for axis in 0..3 {
        let (len, stride) = match axis {
            0 => (g.nx, 1),
            1 => (g.ny, g.nx),
            _ => (g.nz, g.nx * g.ny),
        };
        tmp.fill(false);
        for id in 0..occupied.len() {
            if !occupied[id] {
                continue;
            }
            let pos = (id / stride) % len;
            let lo = pos.saturating_sub(r);
            let hi = (pos + r).min(len - 1);
            for q in lo..=hi {
                tmp[id - pos * stride + q * stride] = true;
            }
        }
        occupied.copy_from_slice(&tmp);
    }
}

/// Malla de la nube por Poisson. `voxel` (mm) es el tamaño de celda (menor = más
/// detalle y más memoria); `fill` agranda hasta dónde se confía en el relleno
/// de huecos; `smooth` son pasadas de suavizado laplaciano de la malla.
pub fn reconstruct_poisson(cloud: &PointCloud, voxel: f32, fill: u32, smooth: u32) -> Mesh {
    if cloud.points.len() < 8 {
        return Mesh::default();
    }
    let voxel = voxel.max(0.5);
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for p in &cloud.points {
        for (d, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            mn[d] = mn[d].min(v);
            mx[d] = mx[d].max(v);
        }
    }
    let origin = [mn[0] - PAD as f32 * voxel, mn[1] - PAD as f32 * voxel, mn[2] - PAD as f32 * voxel];
    let dim = |lo: f32, hi: f32| ((hi - lo) / voxel).ceil() as usize + 1 + 2 * PAD;
    let g = Grid { nx: dim(mn[0], mx[0]).max(4), ny: dim(mn[1], mx[1]).max(4), nz: dim(mn[2], mx[2]).max(4) };
    if g.len() > MAX_NODES {
        return Mesh::default();
    }
    let n = g.len();

    let pos: Vec<[f32; 3]> = cloud.points.iter().map(|p| [p.x, p.y, p.z]).collect();
    let views: Vec<[f32; 3]> = cloud.points.iter().map(|p| p.view).collect();
    let mut nrm = crate::scan::estimate_normals(&pos, (2.5 * voxel).max(3.0));
    orient_normals(&pos, &views, &mut nrm);

    // Campo de vectores: la normal (hacia afuera) con signo cambiado, repartida
    // entre los 8 nodos que rodean a cada punto
    let mut v = [vec![0f32; n], vec![0f32; n], vec![0f32; n]];
    let mut occupied = vec![false; n];
    let mut csum = vec![[0f32; 3]; n];
    let mut ccount = vec![0f32; n];
    let grid_pos = |p: &[f32; 3]| [(p[0] - origin[0]) / voxel, (p[1] - origin[1]) / voxel, (p[2] - origin[2]) / voxel];
    for (pi, p) in pos.iter().enumerate() {
        let q = grid_pos(p);
        let (i0, j0, k0) = (q[0].floor() as usize, q[1].floor() as usize, q[2].floor() as usize);
        let (fx, fy, fz) = (q[0] - i0 as f32, q[1] - j0 as f32, q[2] - k0 as f32);
        for dk in 0..2 {
            for dj in 0..2 {
                for di in 0..2 {
                    let w = (if di == 1 { fx } else { 1.0 - fx })
                        * (if dj == 1 { fy } else { 1.0 - fy })
                        * (if dk == 1 { fz } else { 1.0 - fz });
                    let id = g.idx(i0 + di, j0 + dj, k0 + dk);
                    for d in 0..3 {
                        v[d][id] -= w * nrm[pi][d];
                    }
                }
            }
        }
        let near = g.idx(q[0].round() as usize, q[1].round() as usize, q[2].round() as usize);
        occupied[near] = true;
        let c = cloud.points[pi].rgb;
        for d in 0..3 {
            csum[near][d] += c[d] as f32;
        }
        ccount[near] += 1.0;
    }

    // Divergencia por diferencias centradas
    let mut rhs = vec![0f32; n];
    for k in 1..g.nz - 1 {
        for j in 1..g.ny - 1 {
            for i in 1..g.nx - 1 {
                let id = g.idx(i, j, k);
                rhs[id] = 0.5
                    * ((v[0][id + 1] - v[0][id - 1]) + (v[1][id + g.nx] - v[1][id - g.nx]) + (v[2][id + g.nx * g.ny] - v[2][id - g.nx * g.ny]));
            }
        }
    }
    // Δχ = ∇·V  ⇒  -Δχ = -∇·V
    for r in rhs.iter_mut() {
        *r = -*r;
    }
    let chi = solve(&g, &rhs, 400);

    // Valor de la superficie: el promedio de χ en los puntos
    let mut iso = 0.0f64;
    for p in &pos {
        let q = grid_pos(p);
        let (i0, j0, k0) = (q[0].floor() as usize, q[1].floor() as usize, q[2].floor() as usize);
        let (fx, fy, fz) = (q[0] - i0 as f32, q[1] - j0 as f32, q[2] - k0 as f32);
        let mut s = 0.0f32;
        for dk in 0..2 {
            for dj in 0..2 {
                for di in 0..2 {
                    let w = (if di == 1 { fx } else { 1.0 - fx })
                        * (if dj == 1 { fy } else { 1.0 - fy })
                        * (if dk == 1 { fz } else { 1.0 - fz });
                    s += w * chi[g.idx(i0 + di, j0 + dj, k0 + dk)];
                }
            }
        }
        iso += s as f64;
    }
    let iso = (iso / pos.len() as f64) as f32;

    // Solo cerca de los datos: Poisson cierra los huecos, pero no debe inventar
    // superficie lejos de lo escaneado
    let mut near = occupied.clone();
    dilate(&g, &mut near, 2 + fill as usize);

    // Color: se extiende a los nodos vecinos sin puntos
    let mut color = vec![[200u8; 3]; n];
    let mut known = vec![false; n];
    for id in 0..n {
        if ccount[id] > 0.0 {
            known[id] = true;
            color[id] = [(csum[id][0] / ccount[id]) as u8, (csum[id][1] / ccount[id]) as u8, (csum[id][2] / ccount[id]) as u8];
        }
    }
    for _ in 0..(3 + fill as usize) {
        let (prev_color, prev_known) = (color.clone(), known.clone());
        for k in 1..g.nz - 1 {
            for j in 1..g.ny - 1 {
                for i in 1..g.nx - 1 {
                    let id = g.idx(i, j, k);
                    if prev_known[id] {
                        continue;
                    }
                    let mut acc = [0u32; 3];
                    let mut cnt = 0u32;
                    for nb in [id - 1, id + 1, id - g.nx, id + g.nx, id - g.nx * g.ny, id + g.nx * g.ny] {
                        if prev_known[nb] {
                            for d in 0..3 {
                                acc[d] += prev_color[nb][d] as u32;
                            }
                            cnt += 1;
                        }
                    }
                    if cnt > 0 {
                        known[id] = true;
                        color[id] = [(acc[0] / cnt) as u8, (acc[1] / cnt) as u8, (acc[2] / cnt) as u8];
                    }
                }
            }
        }
    }

    // Surface Nets sobre f = χ - iso (adentro: f > 0)
    let f = |id: usize| chi[id] - iso;
    let corners: [[usize; 3]; 8] = [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]];
    let edges: [(usize, usize); 12] = [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
    let mut mesh = Mesh::default();
    let mut cell_vert: HashMap<(usize, usize, usize), u32> = HashMap::new();
    for k in 0..g.nz - 1 {
        for j in 0..g.ny - 1 {
            for i in 0..g.nx - 1 {
                if !near[g.idx(i, j, k)] {
                    continue;
                }
                let at = |c: usize| g.idx(i + corners[c][0], j + corners[c][1], k + corners[c][2]);
                let mut mask = 0u8;
                for c in 0..8 {
                    if f(at(c)) > 0.0 {
                        mask |= 1 << c;
                    }
                }
                if mask == 0 || mask == 0xFF {
                    continue;
                }
                let mut acc = [0f32; 3];
                let mut cnt = 0f32;
                for &(a, b) in &edges {
                    let (fa, fb) = (f(at(a)), f(at(b)));
                    if (fa > 0.0) != (fb > 0.0) {
                        let t = fa / (fa - fb);
                        for d in 0..3 {
                            acc[d] += corners[a][d] as f32 + t * (corners[b][d] as f32 - corners[a][d] as f32);
                        }
                        cnt += 1.0;
                    }
                }
                let at_cell = [i as f32 + acc[0] / cnt, j as f32 + acc[1] / cnt, k as f32 + acc[2] / cnt];
                mesh.vertices.push([
                    origin[0] + at_cell[0] * voxel,
                    origin[1] + at_cell[1] * voxel,
                    origin[2] + at_cell[2] * voxel,
                ]);
                mesh.colors.push(color[at(0)]);
                cell_vert.insert((i, j, k), (mesh.vertices.len() - 1) as u32);
            }
        }
    }
    let inside = |i: usize, j: usize, k: usize| f(g.idx(i, j, k)) > 0.0;
    let gather = |cells: [(usize, usize, usize); 4]| -> Option<[u32; 4]> {
        Some([*cell_vert.get(&cells[0])?, *cell_vert.get(&cells[1])?, *cell_vert.get(&cells[2])?, *cell_vert.get(&cells[3])?])
    };
    for k in 1..g.nz - 1 {
        for j in 1..g.ny - 1 {
            for i in 0..g.nx - 1 {
                if inside(i, j, k) != inside(i + 1, j, k) {
                    if let Some(q) = gather([(i, j - 1, k - 1), (i, j, k - 1), (i, j - 1, k), (i, j, k)]) {
                        push_quad(&mut mesh.tris, q, inside(i, j, k));
                    }
                }
            }
        }
    }
    for k in 1..g.nz - 1 {
        for j in 0..g.ny - 1 {
            for i in 1..g.nx - 1 {
                if inside(i, j, k) != inside(i, j + 1, k) {
                    if let Some(q) = gather([(i - 1, j, k - 1), (i, j, k - 1), (i - 1, j, k), (i, j, k)]) {
                        push_quad(&mut mesh.tris, q, inside(i, j + 1, k));
                    }
                }
            }
        }
    }
    for k in 0..g.nz - 1 {
        for j in 1..g.ny - 1 {
            for i in 1..g.nx - 1 {
                if inside(i, j, k) != inside(i, j, k + 1) {
                    if let Some(q) = gather([(i - 1, j - 1, k), (i, j - 1, k), (i - 1, j, k), (i, j, k)]) {
                        push_quad(&mut mesh.tris, q, inside(i, j, k));
                    }
                }
            }
        }
    }
    laplacian_smooth(&mut mesh, smooth);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointcloud::Point;

    /// Esfera de 20 mm de radio con ~3 000 puntos (espiral de Fibonacci), vista desde afuera
    fn sphere(radius: f32) -> PointCloud {
        let n = 3000;
        let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
        let points = (0..n)
            .map(|i| {
                let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
                let r = (1.0 - y * y).sqrt();
                let a = golden * i as f32;
                let d = [r * a.cos(), y, r * a.sin()];
                Point { x: d[0] * radius, y: d[1] * radius, z: d[2] * radius, rgb: [180, 90, 40], view: d }
            })
            .collect();
        PointCloud { points, has_color: true }
    }

    #[test]
    fn la_esfera_sale_cerrada_y_del_tamano_correcto() {
        let radius = 20.0;
        let mesh = reconstruct_poisson(&sphere(radius), 2.0, 0, 1);
        assert!(mesh.tris.len() > 300, "{} triángulos", mesh.tris.len());
        for v in &mesh.vertices {
            let d = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((d - radius).abs() < 2.5, "vértice a {d} mm del centro");
        }
        // Volumen con signo (positivo = normales hacia afuera) cerca del de la esfera
        let volume: f64 = mesh
            .tris
            .iter()
            .map(|t| {
                let [a, b, c] = [mesh.vertices[t[0] as usize], mesh.vertices[t[1] as usize], mesh.vertices[t[2] as usize]];
                let cr = [b[1] * c[2] - b[2] * c[1], b[2] * c[0] - b[0] * c[2], b[0] * c[1] - b[1] * c[0]];
                (a[0] * cr[0] + a[1] * cr[1] + a[2] * cr[2]) as f64 / 6.0
            })
            .sum();
        let ideal = 4.0 / 3.0 * std::f64::consts::PI * (radius as f64).powi(3);
        assert!((volume / ideal - 1.0).abs() < 0.2, "volumen {volume} contra {ideal}");
    }

    #[test]
    fn pocos_puntos_no_dan_malla() {
        let cloud = PointCloud { points: sphere(10.0).points.into_iter().take(4).collect(), has_color: true };
        assert!(reconstruct_poisson(&cloud, 2.0, 0, 0).is_empty());
    }
}
