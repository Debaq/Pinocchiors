use std::f64::consts::TAU;
use uv_core::{unwrap, Unwrap, UnwrapOptions};

type Quads = (Vec<[f64; 3]>, Vec<[usize; 4]>);

/// Rejilla `(n+1) × (m+1)` de vértices dados por `f(i, j)`; `wrap_i`/`wrap_j`
/// cierran la rejilla en esa dirección.
fn grid(n: usize, m: usize, wrap_i: bool, wrap_j: bool, f: impl Fn(usize, usize) -> [f64; 3]) -> Quads {
    let (ni, nj) = (if wrap_i { n } else { n + 1 }, if wrap_j { m } else { m + 1 });
    let points = (0..nj).flat_map(|j| (0..ni).map(move |i| (i, j))).map(|(i, j)| f(i, j)).collect();
    let idx = |i: usize, j: usize| (j % nj) * ni + (i % ni);
    let faces = (0..m)
        .flat_map(|j| (0..n).map(move |i| (i, j)))
        .map(|(i, j)| [idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1)])
        .collect();
    (points, faces)
}

/// Cubo de lado 2 con `n × n` quads por cara, opcionalmente inflado a esfera.
fn cube(n: usize, sphere: bool) -> Quads {
    let mut points: Vec<[f64; 3]> = Vec::new();
    let mut faces = Vec::new();
    let mut index = std::collections::HashMap::new();
    let mut vertex = |p: [f64; 3], points: &mut Vec<[f64; 3]>| {
        let key = p.map(|c| (c * 1e6).round() as i64);
        *index.entry(key).or_insert_with(|| {
            let q = if sphere {
                let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
                p.map(|c| c / len)
            } else {
                p
            };
            points.push(q);
            points.len() - 1
        })
    };
    // Cada cara: normal a lo largo del eje `axis` con signo `sign`
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let at = |i: usize, j: usize| {
                let mut p = [0.0; 3];
                p[axis] = sign;
                p[u] = -1.0 + 2.0 * i as f64 / n as f64;
                p[v] = -1.0 + 2.0 * j as f64 / n as f64;
                p
            };
            for j in 0..n {
                for i in 0..n {
                    let mut q = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)].map(|p| vertex(p, &mut points));
                    if sign < 0.0 {
                        q.reverse();
                    }
                    faces.push(q);
                }
            }
        }
    }
    (points, faces)
}

/// Texels cubiertos por más de una carta o por dos triángulos de la misma
/// carta que se solapan.
fn overlapping_texels<const N: usize>(result: &Unwrap<N>, resolution: usize) -> usize {
    let mut owner: Vec<Option<(usize, usize)>> = vec![None; resolution * resolution];
    let mut overlaps = 0;
    let mut tri_id = 0;
    for (face, uvs) in result.corners.iter().enumerate() {
        for k in 1..N - 1 {
            let t = [uvs[0], uvs[k], uvs[k + 1]].map(|p| [p[0] as f64 * resolution as f64, p[1] as f64 * resolution as f64]);
            let area = (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
            let (x0, x1) = (t.iter().map(|p| p[0]).fold(f64::MAX, f64::min), t.iter().map(|p| p[0]).fold(f64::MIN, f64::max));
            let (y0, y1) = (t.iter().map(|p| p[1]).fold(f64::MAX, f64::min), t.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
            for y in (y0.floor().max(0.0) as usize)..(y1.ceil() as usize).min(resolution) {
                for x in (x0.floor().max(0.0) as usize)..(x1.ceil() as usize).min(resolution) {
                    let p = [x as f64 + 0.5, y as f64 + 0.5];
                    let edge = |a: [f64; 2], b: [f64; 2]| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) * area.signum();
                    // Interior estricto: los bordes compartidos no cuentan
                    if edge(t[0], t[1]) > 1e-9 && edge(t[1], t[2]) > 1e-9 && edge(t[2], t[0]) > 1e-9 {
                        let cell = &mut owner[y * resolution + x];
                        match cell {
                            Some((chart, other)) if *chart != result.chart_of_face[face] || *other != tri_id => overlaps += 1,
                            _ => *cell = Some((result.chart_of_face[face], tri_id)),
                        }
                    }
                }
            }
            tri_id += 1;
        }
    }
    overlaps
}

fn check<const N: usize>(result: &Unwrap<N>, max_stretch: f64) {
    assert_eq!(result.flipped, 0, "triángulos invertidos");
    assert!(result.stretch <= max_stretch, "estiramiento {}", result.stretch);
    assert!(
        result.corners.iter().flatten().flatten().all(|&c| (0.0..=1.0).contains(&c)),
        "UV fuera de [0, 1]"
    );
    assert_eq!(overlapping_texels(result, 512), 0, "cartas solapadas");
}

#[test]
fn flat_grid_is_one_isometric_chart() {
    let (points, faces) = grid(10, 6, false, false, |i, j| [i as f64 * 0.3, j as f64 * 0.3, 0.0]);
    let result = unwrap(&points, &faces, &UnwrapOptions::default());
    assert_eq!(result.num_charts, 1);
    check(&result, 1.001);
    assert!(result.coverage > 0.4, "cobertura {}", result.coverage);
}

#[test]
fn cube_splits_at_sharp_edges() {
    let (points, faces) = cube(4, false);
    let result = unwrap(&points, &faces, &UnwrapOptions::default());
    assert_eq!(result.num_charts, 6, "una carta por cara del cubo");
    check(&result, 1.001);
    // Cada carta es una cara entera del cubo
    for side in 0..6 {
        let charts: std::collections::HashSet<usize> = (0..16).map(|k| result.chart_of_face[side * 16 + k]).collect();
        assert_eq!(charts.len(), 1);
    }
}

#[test]
fn sphere_unwraps_without_flips() {
    let (points, faces) = cube(12, true);
    let result = unwrap(&points, &faces, &UnwrapOptions::default());
    assert!(result.num_charts >= 2 && result.num_charts <= 40, "{} cartas", result.num_charts);
    check(&result, UnwrapOptions::default().max_stretch);
}

#[test]
fn torus_charts_are_disks() {
    let (points, faces) = grid(32, 16, true, true, |i, j| {
        let (a, b) = (TAU * i as f64 / 32.0, TAU * j as f64 / 16.0);
        let r = 1.0 + 0.35 * b.cos();
        [r * a.cos(), r * a.sin(), 0.35 * b.sin()]
    });
    let result = unwrap(&points, &faces, &UnwrapOptions::default());
    check(&result, UnwrapOptions::default().max_stretch);
}
