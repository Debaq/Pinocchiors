//! Benchmarks de rendimiento de pinocchio-repair (reparación de mallas).
//!
//! No son pruebas de corrección: miden tiempos y los imprimen con el formato
//! fijo `PERF crate=<crate> bench=<nombre> threads=<n> tris=<n> ms=<mediana>`.
//! Cada uno verifica además que el resultado sea sensato (assert barato).
//!
//! Ejecutar (sin LTO: el perfil release del repo tarda muchísimo en enlazar).
//! La app compila este crate con `parallel`, así que se mide con esa feature:
//!
//! ```text
//! CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
//!   cargo test --release -p pinocchio-repair --features parallel --lib perf_tests \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! - `--test-threads=1`: los benchmarks no deben competir entre sí.
//! - `PERF_SCALE=small`: tamaño diminuto, solo para humo. Por defecto: ~200k
//!   triángulos con ~600 agujeros.
//! - Escalado con hilos: correr con `RAYON_NUM_THREADS=1` y con el valor por
//!   defecto (sin la feature `parallel`, `threads=1`).
//! - Medición: 1 calentamiento + mediana de 3. Si el calentamiento tarda más de
//!   20 s se informa ese único tiempo (1 repetición).
//!
//! La malla de prueba es una esfera UV con relieve a la que se le abren agujeros
//! (de radio 1 a 3 anillos), se le agregan caras repetidas, caras volteadas y
//! "aletas" (aristas con 3 caras), y se pasa a sopa de triángulos (cada cara con
//! sus tres vértices, como un STL) para que haya que soldar.

use crate::repair::{cleanup, holes, manifold};
use crate::topology::{boundary_loops, EdgeTopology};
use crate::{analyze_trimesh, repair_trimesh, AnalysisConfig, HoleFillConfig, RepairConfig, TriMesh};
use pinocchio_math::Vector3;
use std::time::Instant;

const CRATE: &str = "pinocchio-repair";

fn small() -> bool {
    std::env::var("PERF_SCALE").is_ok_and(|s| s == "small")
}

fn threads() -> usize {
    #[cfg(feature = "parallel")]
    {
        rayon::current_num_threads()
    }
    #[cfg(not(feature = "parallel"))]
    {
        1
    }
}

/// Calentamiento + mediana de 3 (1 sola si el calentamiento pasa de 20 s).
/// `setup` prepara la entrada de cada corrida fuera del cronómetro; `run` la
/// consume. Devuelve el resultado de la última corrida.
fn bench<I, T>(name: &str, tris: usize, mut setup: impl FnMut() -> I, mut run: impl FnMut(I) -> T) -> T {
    let input = setup();
    let t0 = Instant::now();
    let mut last = run(input);
    let warm = t0.elapsed().as_secs_f64() * 1e3;
    let ms = if warm > 20_000.0 {
        warm
    } else {
        let mut times = Vec::new();
        for _ in 0..3 {
            let input = setup();
            let t = Instant::now();
            last = run(input);
            times.push(t.elapsed().as_secs_f64() * 1e3);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        times[1]
    };
    println!("PERF crate={CRATE} bench={name} threads={} tris={tris} ms={ms:.2}", threads());
    last
}

/// La malla rota y lo que se espera de ella
struct Broken {
    /// Sopa de triángulos (3 vértices por cara)
    soup: TriMesh,
    /// Vértices de la esfera original (los de la sopa soldada)
    base_vertices: usize,
    /// Agujeros abiertos
    holes: usize,
}

fn broken_sphere() -> Broken {
    let (rings, segments) = if small() { (40, 48) } else { (250, 396) };
    // Esfera UV con relieve
    let mut p = vec![Vector3::new(0.0, 1.0, 0.0)];
    for r in 1..rings {
        let phi = std::f64::consts::PI * r as f64 / rings as f64;
        for s in 0..segments {
            let theta = std::f64::consts::TAU * s as f64 / segments as f64;
            let k = 1.0 + 0.05 * (5.0 * theta).sin() * phi.sin() + 0.02 * (13.0 * phi).cos() * (7.0 * theta).sin();
            p.push(Vector3::new(k * phi.sin() * theta.cos(), k * phi.cos(), k * phi.sin() * theta.sin()));
        }
    }
    p.push(Vector3::new(0.0, -1.0, 0.0));
    let bottom = p.len() - 1;
    let ring = |r: usize, s: usize| 1 + (r - 1) * segments + s % segments;
    let mut t: Vec<[usize; 3]> = Vec::new();
    for s in 0..segments {
        t.push([0, ring(1, s + 1), ring(1, s)]);
        t.push([bottom, ring(rings - 1, s), ring(rings - 1, s + 1)]);
        for r in 1..rings - 1 {
            let (a, b, c, d) = (ring(r, s), ring(r, s + 1), ring(r + 1, s), ring(r + 1, s + 1));
            t.push([a, b, d]);
            t.push([a, d, c]);
        }
    }

    // Agujeros en una rejilla de centros cada 12 anillos/meridianos; el radio
    // (1..=3) varía con la posición. Un vértice cae en un agujero si está a
    // ≤ radio (en anillos y meridianos) del centro.
    let hole_rows = (rings.saturating_sub(20)) / 12 + 1;
    let hole_cols = segments / 12;
    let in_hole = |v: usize| -> bool {
        if v == 0 || v == bottom {
            return false;
        }
        let (r, s) = (1 + (v - 1) / segments, (v - 1) % segments);
        if r < 10 {
            return false;
        }
        let hr = (r - 10 + 6) / 12;
        let rc = 10 + 12 * hr;
        if hr >= hole_rows || rc + 10 > rings {
            return false;
        }
        let hc = (s + 6) / 12;
        let sc = 12 * hc;
        let ds = s.abs_diff(sc).min(segments - s.abs_diff(sc));
        let rad = (hr + hc) % 3 + 1;
        r.abs_diff(rc) <= rad && ds <= rad && hc <= hole_cols
    };
    let holes = {
        let mut n = 0;
        for hr in 0..hole_rows {
            if 10 + 12 * hr + 10 <= rings {
                n += hole_cols;
            }
        }
        n
    };
    let mut faces: Vec<[usize; 3]> = t.iter().copied().filter(|f| !f.iter().any(|&v| in_hole(v))).collect();

    // Defectos: volteadas (1 de cada 97), repetidas (1 de cada 211) y aletas
    // (1 de cada 5003: un triángulo extra pegado a una arista, que queda con 3 caras)
    let base_count = faces.len();
    let mut extra = Vec::new();
    for (i, f) in faces.iter_mut().enumerate() {
        if i % 97 == 0 {
            f.swap(1, 2);
        }
    }
    for (i, f) in faces.iter().enumerate().take(base_count) {
        if i % 211 == 0 {
            extra.push(*f);
        }
    }
    let mut positions = p.clone();
    for (i, f) in faces.iter().enumerate().take(base_count) {
        if i % 5003 == 0 {
            let apex = positions[f[0]] + Vector3::new(0.02, 0.02, 0.02);
            positions.push(apex);
            extra.push([f[0], f[1], positions.len() - 1]);
        }
    }
    faces.extend(extra);

    // A sopa: cada cara con sus propios vértices
    let mut soup_p = Vec::with_capacity(faces.len() * 3);
    let mut soup_t = Vec::with_capacity(faces.len());
    for f in &faces {
        let base = soup_p.len();
        soup_p.extend(f.iter().map(|&v| positions[v]));
        soup_t.push([base, base + 1, base + 2]);
    }
    Broken { soup: TriMesh::new(soup_p, soup_t), base_vertices: positions.len(), holes }
}

/// Máxima valencia de arista (2 = sin aristas non-manifold) y aristas de borde
fn edge_stats(mesh: &TriMesh) -> (usize, usize) {
    let topo = EdgeTopology::build(&mesh.triangles);
    let max_valence = (0..topo.num_edges()).map(|e| topo.valence(e)).max().unwrap_or(0);
    let boundary = (0..topo.num_edges()).filter(|&e| topo.valence(e) == 1).count();
    (max_valence, boundary)
}

#[test]
#[ignore = "perf"]
fn perf_weld_vertices() {
    let b = broken_sphere();
    let tris = b.soup.num_faces();
    let tol = 1e-6 * b.soup.diagonal();
    let m = bench("weld_vertices", tris, || b.soup.clone(), |mut m| {
        let r = cleanup::weld_vertices(&mut m, tol);
        (m, r)
    });
    let (mesh, (merged, _)) = m;
    assert!(merged > 0);
    // Tras soldar, las caras usan (a lo sumo) los vértices de la esfera original
    let mut used = vec![false; mesh.num_vertices()];
    mesh.triangles.iter().flatten().for_each(|&v| used[v] = true);
    let n_used = used.iter().filter(|&&u| u).count();
    assert!(n_used <= b.base_vertices, "{n_used} vértices usados, esperaba ≤ {}", b.base_vertices);
}

/// Soldada, sin degeneradas ni repetidas: la entrada de los pasos siguientes
fn welded() -> (Broken, TriMesh) {
    let b = broken_sphere();
    let mut m = b.soup.clone();
    let diag = m.diagonal();
    cleanup::weld_vertices(&mut m, 1e-6 * diag);
    cleanup::remove_duplicate_faces(&mut m);
    (b, m)
}

#[test]
#[ignore = "perf"]
fn perf_orient_and_split_non_manifold() {
    let (_, w) = welded();
    let tris = w.num_faces();
    assert!(edge_stats(&w).0 > 2, "la malla de prueba debería tener aristas non-manifold");
    let (m, report) = bench("orient_and_split", tris, || w.clone(), |mut m| {
        let r = manifold::orient_and_split(&mut m, true, true);
        (m, r)
    });
    assert!(report.faces_flipped > 0, "había caras volteadas");
    assert!(edge_stats(&m).0 <= 2, "no deben quedar aristas con más de 2 caras");
}

#[test]
#[ignore = "perf"]
fn perf_fill_holes_liepa_fair() {
    let (b, mut w) = welded();
    manifold::orient_and_split(&mut w, true, true);
    let tris = w.num_faces();
    let cfg = HoleFillConfig::default(); // refine + fair (Liepa)
    let (m, report) = bench("fill_holes_liepa_fair", tris, || w.clone(), |mut m| {
        let r = holes::fill_holes(&mut m, &cfg);
        (m, r)
    });
    assert!(report.filled >= b.holes, "rellenó {} de {} agujeros", report.filled, b.holes);
    assert!(report.faces_added > 0);
    let topo = EdgeTopology::build(&m.triangles);
    assert!(boundary_loops(&m.triangles, &topo).len() <= report.skipped, "quedaron agujeros abiertos");
}

#[test]
#[ignore = "perf"]
fn perf_repair_full_pipeline() {
    let b = broken_sphere();
    let tris = b.soup.num_faces();
    let cfg = RepairConfig { fill_holes: true, ..Default::default() };
    let (m, summary) = bench("repair_full_fill_holes", tris, || b.soup.clone(), |mut m| {
        let s = repair_trimesh(&mut m, &cfg).unwrap();
        (m, s)
    });
    assert!(summary.any_repairs());
    assert!(summary.holes_filled >= b.holes, "{summary:#?}");
    let (max_valence, boundary) = edge_stats(&m);
    assert!(max_valence <= 2, "quedó non-manifold");
    assert!(boundary == 0 || summary.holes_skipped > 0, "{boundary} aristas de borde sin explicación");
}

#[test]
#[ignore = "perf"]
fn perf_analyze() {
    let (_, mut w) = welded();
    manifold::orient_and_split(&mut w, true, true);
    let tris = w.num_faces();
    let cfg = AnalysisConfig::default();
    let d = bench("analyze_trimesh", tris, || (), |_| analyze_trimesh(&w, &cfg));
    assert_eq!(d.num_faces, tris);
    assert!(d.boundary_loops > 0, "la malla tiene agujeros");
}
