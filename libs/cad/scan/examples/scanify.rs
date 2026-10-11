//! Vuelve «escaneo» una malla limpia, para probar el diseño sobre escaneos sin
//! escáner: triángulos parejos, solo lo que ve un plato giratorio, huecos donde
//! no hubo lectura, ruido sobre la normal, ondulación suave y pose al azar.
//!
//! `cargo run --release -p cad-scan --example scanify -- entrada.(stl|obj|glb|ply|3mf) salida.(stl|obj) [opciones]`
//!
//! Opciones (medidas en mm):
//! - `--triangulos N` (200000) o `--lado L`: densidad del «escaneo».
//! - `--ruido S` (0.04): desvío estándar sobre la normal.
//! - `--ondas A` (0.05): amplitud de la ondulación de baja frecuencia.
//! - `--huecos N` (4) y `--radio-hueco R` (diagonal/40): zonas sin lectura.
//! - `--vistas N` (12): posiciones del plato por cada altura de cámara (0: se ve todo).
//! - `--pose`: gira y mueve al azar; la matriz queda en `salida.pose.txt`.
//! - `--semilla N` (1).

use std::collections::HashMap;
use std::f64::consts::PI;
use std::io::Write;
use std::path::Path;

use quadriflow_core::remesh::{IsotropicOptions, VoxelOptions, isotropic, voxel};

type P3 = [f64; 3];

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn normalize(a: P3) -> P3 {
    let n = norm(a);
    if n > 0.0 { scale(a, 1.0 / n) } else { a }
}

/// splitmix64: repetible con la misma semilla, sin dependencias.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// En [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn gauss(&mut self) -> f64 {
        let u = self.unit().max(1e-300);
        let v = self.unit();
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }
    fn direction(&mut self) -> P3 {
        normalize([self.gauss(), self.gauss(), self.gauss()])
    }
}

struct Args {
    input: String,
    output: String,
    triangles: usize,
    edge: Option<f64>,
    noise: f64,
    waves: f64,
    holes: usize,
    hole_radius: Option<f64>,
    views: usize,
    pose: bool,
    seed: u64,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let usage = "uso: scanify entrada salida [--triangulos N | --lado L] [--ruido S] [--ondas A] [--huecos N] [--radio-hueco R] [--vistas N] [--pose] [--semilla N]";
    let input = it.next().expect(usage);
    let output = it.next().expect(usage);
    let mut a = Args { input, output, triangles: 200_000, edge: None, noise: 0.04, waves: 0.05, holes: 4, hole_radius: None, views: 12, pose: false, seed: 1 };
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| panic!("falta el valor de {flag}"));
        match flag.as_str() {
            "--triangulos" => a.triangles = value().parse().unwrap(),
            "--lado" => a.edge = Some(value().parse().unwrap()),
            "--ruido" => a.noise = value().parse().unwrap(),
            "--ondas" => a.waves = value().parse().unwrap(),
            "--huecos" => a.holes = value().parse().unwrap(),
            "--radio-hueco" => a.hole_radius = Some(value().parse().unwrap()),
            "--vistas" => a.views = value().parse().unwrap(),
            "--semilla" => a.seed = value().parse().unwrap(),
            "--pose" => a.pose = true,
            _ => panic!("opción desconocida {flag}\n{usage}"),
        }
    }
    a
}

/// La malla del archivo en mm y Z arriba, como la toma el CAD (`with_scan`).
fn load(path: &str) -> (Vec<[f32; 3]>, Vec<u32>) {
    let scene = converter_core::import(path).unwrap_or_else(|e| panic!("no se pudo leer {path}: {e}"));
    let mm = (scene.meters_per_unit * 1000.0) as f32;
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for prim in scene.world_primitives_z_up() {
        let base = positions.len() as u32;
        positions.extend(prim.positions.iter().map(|p| p.map(|c| c * mm)));
        indices.extend(prim.triangles.iter().flat_map(|t| t.map(|i| i + base)));
    }
    (positions, indices)
}

/// Suelda vértices iguales (STL trae cada triángulo suelto).
fn weld(positions: &[[f32; 3]], indices: &[u32]) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut map: HashMap<[u32; 3], u32> = HashMap::new();
    let mut out = Vec::new();
    let remap: Vec<u32> = positions
        .iter()
        .map(|p| {
            *map.entry(p.map(f32::to_bits)).or_insert_with(|| {
                out.push(*p);
                (out.len() - 1) as u32
            })
        })
        .collect();
    let tris = indices.chunks(3).filter_map(|t| {
        let t = [remap[t[0] as usize], remap[t[1] as usize], remap[t[2] as usize]];
        (t[0] != t[1] && t[1] != t[2] && t[2] != t[0]).then_some(t)
    });
    (out, tris.flatten().collect())
}

fn area(positions: &[P3], tris: &[[u32; 3]]) -> f64 {
    tris.iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| positions[i as usize]);
            norm(cross(sub(b, a), sub(c, a))) / 2.0
        })
        .sum()
}

/// Caras que ve alguna cámara del plato giratorio: dos alturas (20° y 50°
/// sobre la mesa) y `views` giros del plato en cada una. Visibilidad por
/// buffer de profundidad ortográfico; nada se ve desde abajo de la mesa.
fn visible_faces(positions: &[P3], tris: &[[u32; 3]], views: usize) -> Vec<bool> {
    const RES: usize = 768;
    let mut seen = vec![false; tris.len()];
    let normals: Vec<P3> = tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| positions[i as usize]);
            normalize(cross(sub(b, a), sub(c, a)))
        })
        .collect();
    for elevation in [20f64, 50.0] {
        for k in 0..views {
            let az = 2.0 * PI * k as f64 / views as f64;
            let el = elevation.to_radians();
            // Hacia la cámara
            let toward = [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()];
            let right = normalize(cross([0.0, 0.0, 1.0], toward));
            let up = cross(toward, right);
            let proj: Vec<(f64, f64, f64)> = positions.iter().map(|&p| (dot(p, right), dot(p, up), dot(p, toward))).collect();
            let (mut min_u, mut max_u, mut min_v, mut max_v) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for &(u, v, _) in &proj {
                min_u = min_u.min(u);
                max_u = max_u.max(u);
                min_v = min_v.min(v);
                max_v = max_v.max(v);
            }
            let cell = (max_u - min_u).max(max_v - min_v) / (RES - 1) as f64;
            let px = |u: f64, v: f64| ((u - min_u) / cell, (v - min_v) / cell);
            // Profundidad = cuánto se acerca a la cámara (mayor = más cerca)
            let mut depth = vec![f64::MIN; RES * RES];
            for t in tris {
                let [a, b, c] = t.map(|i| proj[i as usize]);
                let (pa, pb, pc) = (px(a.0, a.1), px(b.0, b.1), px(c.0, c.1));
                let den = (pb.1 - pc.1) * (pa.0 - pc.0) + (pc.0 - pb.0) * (pa.1 - pc.1);
                if den.abs() < 1e-12 {
                    continue;
                }
                let x0 = pa.0.min(pb.0).min(pc.0).floor().max(0.0) as usize;
                let x1 = (pa.0.max(pb.0).max(pc.0).ceil() as usize).min(RES - 1);
                let y0 = pa.1.min(pb.1).min(pc.1).floor().max(0.0) as usize;
                let y1 = (pa.1.max(pb.1).max(pc.1).ceil() as usize).min(RES - 1);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let (fx, fy) = (x as f64, y as f64);
                        let w0 = ((pb.1 - pc.1) * (fx - pc.0) + (pc.0 - pb.0) * (fy - pc.1)) / den;
                        let w1 = ((pc.1 - pa.1) * (fx - pc.0) + (pa.0 - pc.0) * (fy - pc.1)) / den;
                        let w2 = 1.0 - w0 - w1;
                        if w0 >= -1e-6 && w1 >= -1e-6 && w2 >= -1e-6 {
                            let d = &mut depth[y * RES + x];
                            *d = d.max(w0 * a.2 + w1 * b.2 + w2 * c.2);
                        }
                    }
                }
            }
            // Una cara se ve si apunta a la cámara (no rasante) y su centro no
            // queda tapado. Las caras más chicas que una celda se miran en la
            // celda de su centro.
            let tol = 1.5 * cell;
            for (f, t) in tris.iter().enumerate() {
                if seen[f] || dot(normals[f], toward) < 0.15 {
                    continue;
                }
                let [a, b, c] = t.map(|i| proj[i as usize]);
                let (cu, cv, cd) = ((a.0 + b.0 + c.0) / 3.0, (a.1 + b.1 + c.1) / 3.0, (a.2 + b.2 + c.2) / 3.0);
                let (x, y) = px(cu, cv);
                let (x, y) = (x.round() as usize, y.round() as usize);
                if x < RES && y < RES && cd >= depth[y * RES + x] - tol {
                    seen[f] = true;
                }
            }
        }
    }
    seen
}

/// Normales por vértice ponderadas por área.
fn vertex_normals(positions: &[P3], tris: &[[u32; 3]]) -> Vec<P3> {
    let mut n = vec![[0.0; 3]; positions.len()];
    for t in tris {
        let [a, b, c] = t.map(|i| positions[i as usize]);
        let fnrm = cross(sub(b, a), sub(c, a));
        for &i in t {
            n[i as usize] = add(n[i as usize], fnrm);
        }
    }
    n.into_iter().map(normalize).collect()
}

/// Matriz 4×4 por filas de una rotación al azar (cuaternión uniforme) y una
/// traslación de hasta `reach` en cada eje.
fn random_pose(rng: &mut Rng, reach: f64) -> [[f64; 4]; 4] {
    let (u1, u2, u3) = (rng.unit(), rng.unit(), rng.unit());
    let (x, y, z, w) = (
        (1.0 - u1).sqrt() * (2.0 * PI * u2).sin(),
        (1.0 - u1).sqrt() * (2.0 * PI * u2).cos(),
        u1.sqrt() * (2.0 * PI * u3).sin(),
        u1.sqrt() * (2.0 * PI * u3).cos(),
    );
    let t = [(rng.unit() * 2.0 - 1.0) * reach, (rng.unit() * 2.0 - 1.0) * reach, (rng.unit() * 2.0 - 1.0) * reach];
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w), t[0]],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w), t[1]],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y), t[2]],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn write(path: &str, positions: &[P3], tris: &[[u32; 3]]) {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    match ext.as_str() {
        "stl" => {
            let faces = tris.iter().map(|t| {
                let [a, b, c] = t.map(|i| positions[i as usize]);
                let n = normalize(cross(sub(b, a), sub(c, a)));
                let v = |p: P3| stl_io::Vertex::new(p.map(|c| c as f32));
                stl_io::Triangle { normal: stl_io::Normal::new(n.map(|c| c as f32)), vertices: [v(a), v(b), v(c)] }
            });
            stl_io::write_stl(&mut f, faces).unwrap();
        }
        "obj" => {
            writeln!(f, "# scanify: mm, Z arriba").unwrap();
            for p in positions {
                writeln!(f, "v {:.5} {:.5} {:.5}", p[0], p[1], p[2]).unwrap();
            }
            for t in tris {
                writeln!(f, "f {} {} {}", t[0] + 1, t[1] + 1, t[2] + 1).unwrap();
            }
        }
        _ => panic!("la salida tiene que ser .stl u .obj"),
    }
}

fn main() {
    let args = parse_args();
    let mut rng = Rng(args.seed);
    let t0 = std::time::Instant::now();

    let (positions, indices) = load(&args.input);
    let (positions, indices) = weld(&positions, &indices);
    let stats = quadriflow_core::remesh::surface_stats(&positions, &indices);
    println!("entrada: {} triángulos, área {:.0} mm², tamaño {:.1} mm", indices.len() / 3, stats.area, stats.extent);

    // Densidad pareja, como la nube de un escáner (sin aristas vivas)
    let edge = args.edge.unwrap_or_else(|| (stats.area / (3f64.sqrt() / 4.0 * args.triangles as f64)).sqrt());
    let iso = IsotropicOptions { edge_length: edge, sharp_angle: None, iterations: 5, thin_features: false };
    let remeshed = match isotropic(&positions, &indices, &iso) {
        Ok(m) => m,
        Err(e) => {
            println!("isótropo no se pudo ({e}); se rehace por vóxeles");
            let opts = VoxelOptions { voxel_size: edge, smooth_iterations: 0, isotropic_edge: Some(edge), offset: 0.0 };
            voxel(&positions, &indices, &opts).expect("vóxeles")
        }
    };
    let mut pts: Vec<P3> = remeshed.positions.iter().map(|p| p.map(f64::from)).collect();
    let mut tris: Vec<[u32; 3]> = remeshed.indices.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
    let total_area = area(&pts, &tris);
    println!("remallado: lado {edge:.3} mm, {} triángulos", tris.len());

    // Solo lo que ven las cámaras
    if args.views > 0 {
        let seen = visible_faces(&pts, &tris, args.views);
        tris = tris.iter().zip(&seen).filter(|(_, s)| **s).map(|(t, _)| *t).collect();
        println!("oclusión ({} vistas × 2 alturas): queda {:.1} % del área", args.views, area(&pts, &tris) / total_area * 100.0);
    }

    // Huecos: zonas sin lectura (brillos, oscuros) alrededor de caras al azar
    let diag = norm(sub(
        pts.iter().fold([f64::MIN; 3], |m, p| [m[0].max(p[0]), m[1].max(p[1]), m[2].max(p[2])]),
        pts.iter().fold([f64::MAX; 3], |m, p| [m[0].min(p[0]), m[1].min(p[1]), m[2].min(p[2])]),
    ));
    let radius = args.hole_radius.unwrap_or(diag / 40.0);
    if args.holes > 0 && !tris.is_empty() {
        let centers: Vec<P3> = (0..args.holes)
            .map(|_| {
                let t = tris[(rng.unit() * tris.len() as f64) as usize];
                pts[t[0] as usize]
            })
            .collect();
        let before = area(&pts, &tris);
        tris.retain(|t| {
            let c = scale(add(add(pts[t[0] as usize], pts[t[1] as usize]), pts[t[2] as usize]), 1.0 / 3.0);
            // Radio distinto por hueco y borde irregular
            centers.iter().enumerate().all(|(k, h)| {
                let wobble = 1.0 + 0.3 * ((c[0] + c[1] * 1.3 + c[2] * 0.7) / radius * 2.0 + k as f64).sin();
                norm(sub(c, *h)) > radius * (0.6 + 0.8 * ((k * 7919) % 100) as f64 / 100.0) * wobble
            })
        });
        println!("{} huecos de ~{radius:.1} mm: se perdió {:.1} % del área", args.holes, (1.0 - area(&pts, &tris) / before) * 100.0);
    }

    // Ruido sobre la normal + ondulación suave (calibración, registro de tomas)
    let normals = vertex_normals(&pts, &tris);
    let waves: Vec<(P3, f64, f64)> = (0..3).map(|_| (rng.direction(), 2.0 * PI / (diag * (0.2 + 0.3 * rng.unit())), rng.unit() * 2.0 * PI)).collect();
    for (p, n) in pts.iter_mut().zip(&normals) {
        let wave: f64 = waves.iter().map(|(d, k, phase)| (dot(*p, *d) * k + phase).sin()).sum::<f64>() / 3.0;
        *p = add(*p, scale(*n, args.noise * rng.gauss() + args.waves * wave));
    }

    // Quitar los vértices que ya no usa ningún triángulo
    let mut used = vec![u32::MAX; pts.len()];
    let mut compact = Vec::new();
    for t in &mut tris {
        for i in t.iter_mut() {
            if used[*i as usize] == u32::MAX {
                used[*i as usize] = compact.len() as u32;
                compact.push(pts[*i as usize]);
            }
            *i = used[*i as usize];
        }
    }
    pts = compact;

    if args.pose {
        let m = random_pose(&mut rng, diag / 2.0);
        for p in &mut pts {
            let q = *p;
            *p = [0, 1, 2].map(|r| m[r][0] * q[0] + m[r][1] * q[1] + m[r][2] * q[2] + m[r][3]);
        }
        let pose_path = format!("{}.pose.txt", args.output);
        let text: String = m.iter().map(|r| format!("{:.9} {:.9} {:.9} {:.9}\n", r[0], r[1], r[2], r[3])).collect();
        std::fs::write(&pose_path, text).unwrap();
        println!("pose al azar en {pose_path}");
    }

    write(&args.output, &pts, &tris);
    println!("salida: {} triángulos, {} vértices → {} ({:.1} s)", tris.len(), pts.len(), args.output, t0.elapsed().as_secs_f64());
}
