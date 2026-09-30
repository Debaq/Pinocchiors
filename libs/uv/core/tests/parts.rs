use std::f64::consts::TAU;
use uv_core::{unwrap_by_parts, Layout, Unwrap, UnwrapOptions};

type Quads = (Vec<[f64; 3]>, Vec<[usize; 4]>);

fn torus(n: usize, m: usize) -> Quads {
    let points = (0..m)
        .flat_map(|j| (0..n).map(move |i| (i, j)))
        .map(|(i, j)| {
            let (a, b) = (TAU * i as f64 / n as f64, TAU * j as f64 / m as f64);
            let r = 1.0 + 0.3 * b.cos();
            // Anillo acostado: el eje del toro es Y (arriba)
            [r * a.cos(), 0.3 * b.sin(), r * a.sin()]
        })
        .collect();
    let idx = |i: usize, j: usize| (j % m) * n + (i % n);
    let faces = (0..m).flat_map(|j| (0..n).map(move |i| [idx(i, j), idx(i, j + 1), idx(i + 1, j + 1), idx(i + 1, j)])).collect();
    (points, faces)
}

fn centroid(points: &[[f64; 3]], face: &[usize; 4]) -> [f64; 3] {
    [0, 1, 2].map(|k| face.iter().map(|&v| points[v][k]).sum::<f64>() / 4.0)
}

/// Aristas entre caras de una misma parte que quedaron en costura (UV distintas
/// en algún extremo): (cara, cara).
fn seams_inside_parts(faces: &[[usize; 4]], parts: &[usize], result: &Unwrap<4>) -> Vec<(usize, usize)> {
    let mut edges: std::collections::HashMap<(usize, usize), Vec<(usize, usize)>> = Default::default();
    for (f, q) in faces.iter().enumerate() {
        for k in 0..4 {
            let (a, b) = (q[k], q[(k + 1) % 4]);
            edges.entry((a.min(b), a.max(b))).or_default().push((f, k));
        }
    }
    let uv = |f: usize, v: usize| {
        let k = faces[f].iter().position(|&x| x == v).unwrap();
        result.corners[f][k]
    };
    edges
        .iter()
        .filter_map(|(&(a, b), users)| match users[..] {
            [(f, _), (g, _)] if parts[f] == parts[g] && (uv(f, a) != uv(g, a) || uv(f, b) != uv(g, b)) => Some((f, g)),
            _ => None,
        })
        .collect()
}

#[test]
fn torus_segments_open_on_the_inner_side() {
    let (points, faces) = torus(48, 16);
    // Cuatro tramos del anillo: cada uno es un tubo con dos bordes
    let parts: Vec<usize> = faces
        .iter()
        .map(|q| {
            let c = centroid(&points, q);
            ((c[2].atan2(c[0]) + TAU) % TAU / (TAU / 4.0)) as usize
        })
        .collect();
    let options = UnwrapOptions { layout: Layout::Paintable, texture_size: 1024, ..Default::default() };
    let result = unwrap_by_parts(&points, &faces, &parts, None, &options);
    assert_eq!(result.flipped, 0);
    assert_eq!(result.num_charts, 4, "una carta por tramo");

    // El corte que abre cada tubo va por dentro del anillo (hacia el centro)
    let seams = seams_inside_parts(&faces, &parts, &result);
    assert!(!seams.is_empty());
    for (f, g) in seams {
        let c = [0, 1, 2].map(|k| (centroid(&points, &faces[f])[k] + centroid(&points, &faces[g])[k]) / 2.0);
        let radial = (c[0] * c[0] + c[2] * c[2]).sqrt();
        assert!(radial < 1.0, "costura por fuera del anillo (radio {radial:.2})");
    }
}

#[test]
fn deep_parts_open_with_a_zip() {
    // Esfera: casquete arriba y "media" (todo lo demás, muy honda)
    let n = 40;
    let points: Vec<[f64; 3]> = (0..=n)
        .flat_map(|j| (0..n).map(move |i| (i, j)))
        .map(|(i, j)| {
            let (a, b) = (TAU * i as f64 / n as f64, std::f64::consts::PI * (0.02 + 0.96 * j as f64 / n as f64));
            [b.sin() * a.cos(), b.cos(), b.sin() * a.sin()]
        })
        .collect();
    let idx = |i: usize, j: usize| j * n + (i % n);
    let faces: Vec<[usize; 4]> = (0..n).flat_map(|j| (0..n).map(move |i| [idx(i, j), idx(i, j + 1), idx(i + 1, j + 1), idx(i + 1, j)])).collect();
    let parts: Vec<usize> = faces.iter().map(|q| usize::from(centroid(&points, q)[1] < 0.6)).collect();
    let options = UnwrapOptions { layout: Layout::Paintable, texture_size: 1024, ..Default::default() };
    let result = unwrap_by_parts(&points, &faces, &parts, None, &options);
    assert_eq!(result.flipped, 0);
    assert!(result.num_charts <= 4, "{} cartas", result.num_charts);
    // La media se abrió con un cierre (costura dentro de la parte)
    let seams = seams_inside_parts(&faces, &parts, &result);
    assert!(seams.iter().any(|&(f, _)| parts[f] == 1));
    eprintln!("cartas {}, estiramiento {:.3}, atlas {:.1} %", result.num_charts, result.stretch, 100.0 * result.coverage);
}
