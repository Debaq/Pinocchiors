//! El escaneo como cuerpo: la malla lista para convertirla en sólido y hacer
//! booleanas con las piezas (`FeatureKind::ScanBody`).
//!
//! Las mallas de escáner vienen abiertas, con cruces y ruido: OpenCASCADE no
//! cierra el sólido o lo da inválido (ver E0 en PLAN_ESCANEO.md). Se rehacen
//! por vóxeles, que dan una superficie cerrada y manifold y de paso corren la
//! piel la holgura pedida con la distancia exacta, y después por isótropo a
//! la cantidad de triángulos pedida.

use std::collections::HashMap;

use cad_model::geom::{P3, add, cross, dot, norm, scale, sub};
use quadriflow_core::remesh::{IsotropicOptions, VoxelOptions, isotropic, surface_stats, voxel};

/// Cómo preparar el cuerpo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyOptions {
    /// Triángulos aproximados del cuerpo (más: más fiel y booleanas más lentas,
    /// ~0,2 ms por cara).
    pub faces: usize,
    /// Holgura (mm): positiva agranda el escaneo (encajes que no aprieten),
    /// negativa lo achica.
    pub offset: f64,
}

/// Malla cerrada y manifold, orientada hacia afuera.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyMesh {
    pub vertices: Vec<P3>,
    pub triangles: Vec<[u32; 3]>,
    /// Lado de los triángulos (mm): los detalles más chicos se pierden.
    pub edge: f64,
}

/// Menos triángulos no alcanzan para nada que se parezca al escaneo.
pub const MIN_FACES: usize = 500;
/// Más ya no es razonable para booleanas de OpenCASCADE (~10 s cada una).
pub const MAX_FACES: usize = 100_000;

/// Triángulos del isótropo / triángulos equiláteros del lado pedido en la misma área.
const ISOTROPIC_FILL: f64 = 0.75;

/// Prepara el escaneo como cuerpo.
pub fn prepare_body(vertices: &[P3], triangles: &[[u32; 3]], options: &BodyOptions) -> Result<BodyMesh, String> {
    if !(options.offset.is_finite()) {
        return Err("la holgura no es un número".into());
    }
    let faces = options.faces.clamp(MIN_FACES, MAX_FACES);
    let positions: Vec<[f32; 3]> = vertices.iter().map(|p| p.map(|c| c as f32)).collect();
    let indices: Vec<u32> = triangles.iter().flatten().copied().collect();
    let stats = surface_stats(&positions, &indices);
    if !(stats.area > 0.0) {
        return Err("el escaneo no tiene superficie".into());
    }
    let triangle_area = |edge: f64| 3f64.sqrt() / 4.0 * edge * edge;
    // Vóxeles a la mitad del lado que tendría el cuerpo con el área del escaneo
    let rough = (stats.area / triangle_area(1.0) / faces as f64).sqrt();
    let opts = VoxelOptions { voxel_size: rough / 2.0, smooth_iterations: 0, isotropic_edge: None, offset: options.offset };
    let closed = voxel(&positions, &indices, &opts).map_err(|e| match e {
        quadriflow_core::RemeshError::EmptyMesh => "la holgura se comió el escaneo entero".to_string(),
        e => e.to_string(),
    })?;
    // El lado, con el área ya cerrada y corrida (los huecos tapados, el ruido
    // y la holgura la cambian): así salen los triángulos pedidos. El isótropo
    // deja aristas algo más largas que las pedidas: ~¾ de los triángulos
    // equiláteros que cabrían (medido)
    let area = surface_stats(&closed.positions, &closed.indices).area;
    let edge = (ISOTROPIC_FILL * area / triangle_area(1.0) / faces as f64).sqrt();
    // El isótropo a veces deja un agujero de tres vértices en fila (una
    // astilla que se cayó al soldar): se tapa y, si queda alguno sin área, se
    // rehace con un lado apenas distinto. Sin eso OpenCASCADE tiene que
    // coser, ~100 veces más lento.
    let mut last = SolidProblems::default();
    for attempt in 0..3 {
        let edge = edge * (1.0 + 0.013 * attempt as f64);
        let iso = IsotropicOptions { edge_length: edge, sharp_angle: None, ..Default::default() };
        let mesh = isotropic(&closed.positions, &closed.indices, &iso).map_err(|e| e.to_string())?;
        let mut vertices: Vec<P3> = mesh.positions.iter().map(|p| p.map(f64::from)).collect();
        let mut triangles: Vec<[u32; 3]> = mesh.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        drop_specks(&mut triangles, &vertices);
        close_open_edges(&mut vertices, &mut triangles);
        last = solid_problems(&vertices, &triangles);
        if last == SolidProblems::default() {
            return Ok(BodyMesh { vertices, triangles, edge });
        }
    }
    Err(format!("el escaneo no quedó cerrado ({last:?})"))
}

/// Aristas con una sola cara (o con más de dos), contando ambos sentidos.
pub fn open_edges(triangles: &[[u32; 3]]) -> usize {
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    edges.values().filter(|&&n| n != 2).count()
}

/// Lo que impide armar el sólido directo (sin coser): aristas dirigidas
/// repetidas (caras dadas vuelta o de más de dos por arista), aristas sin su
/// vuelta (bordes) y triángulos sin área.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SolidProblems {
    pub repeated: usize,
    pub unmatched: usize,
    pub degenerate: usize,
}

pub fn solid_problems(vertices: &[P3], triangles: &[[u32; 3]]) -> SolidProblems {
    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    let mut out = SolidProblems::default();
    for t in triangles {
        for k in 0..3 {
            let e = directed.entry((t[k], t[(k + 1) % 3])).or_default();
            *e += 1;
            if *e == 2 {
                out.repeated += 1;
            }
        }
        let [a, b, c] = t.map(|i| vertices[i as usize]);
        if norm(cross(sub(b, a), sub(c, a))) < 1e-8 {
            out.degenerate += 1;
        }
    }
    out.unmatched = directed.keys().filter(|(a, b)| !directed.contains_key(&(*b, *a))).count();
    out
}

/// Las motas sueltas (restos de ruido o de partes finas que la holgura
/// negativa se comió) se descartan: menos del 1 % del volumen de la mayor.
fn drop_specks(triangles: &mut Vec<[u32; 3]>, vertices: &[P3]) {
    let mut parent: Vec<u32> = (0..vertices.len() as u32).collect();
    fn root(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for t in triangles.iter() {
        let a = root(&mut parent, t[0]);
        for &k in &t[1..] {
            let b = root(&mut parent, k);
            parent[b as usize] = a;
        }
    }
    let mut volume: HashMap<u32, f64> = HashMap::new();
    let roots: Vec<u32> = triangles.iter().map(|t| root(&mut parent, t[0])).collect();
    for (t, r) in triangles.iter().zip(&roots) {
        let [a, b, c] = t.map(|i| vertices[i as usize]);
        *volume.entry(*r).or_default() += dot(a, cross(b, c)) / 6.0;
    }
    let largest = volume.values().copied().fold(0.0, f64::max);
    let mut k = 0;
    triangles.retain(|_| {
        k += 1;
        volume[&roots[k - 1]] >= largest * 0.01
    });
}

/// El isótropo a veces deja unas pocas aristas sueltas (un colapso que junta
/// dos bordes): sin cerrarlas OpenCASCADE tiene que coser, ~100 veces más
/// lento. Se tapan los agujeros chicos con un abanico desde su centro.
fn close_open_edges(vertices: &mut Vec<P3>, triangles: &mut Vec<[u32; 3]>) {
    // Arista dirigida sin su vuelta = borde de un agujero, recorrido al revés
    let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
    for t in triangles.iter() {
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    let mut next: HashMap<u32, u32> = HashMap::new();
    for (&(a, b), _) in directed.iter().filter(|((a, b), _)| !directed.contains_key(&(*b, *a))) {
        // El agujero se recorre b → a para que las caras nuevas miren afuera
        next.insert(b, a);
    }
    let mut merge: HashMap<u32, u32> = HashMap::new();
    while let Some(&start) = next.keys().next() {
        let mut ring = vec![start];
        let mut at = start;
        while let Some(n) = next.remove(&at) {
            if n == start {
                break;
            }
            ring.push(n);
            at = n;
        }
        if ring.len() < 3 {
            continue;
        }
        let p = |i: u32| vertices[i as usize];
        // Agujero sin área (vértices en fila, una rendija de ida y vuelta: lo
        // que deja una astilla que se cayó al soldar) o microscópico: un
        // abanico tampoco tendría área. Se funde el vértice más llano con su vecino más
        // cercano hasta que el agujero tenga área o desaparezca
        while ring.len() >= 3 && is_flat(&ring, &p) {
            let k = (0..ring.len())
                .min_by(|&i, &j| straightness(&ring, i, &p).total_cmp(&straightness(&ring, j, &p)))
                .unwrap();
            let (m, prev, next) = (ring[k], ring[(k + ring.len() - 1) % ring.len()], ring[(k + 1) % ring.len()]);
            let near = if norm(sub(p(prev), p(m))) < norm(sub(p(next), p(m))) { prev } else { next };
            merge.insert(m, near);
            ring.remove(k);
        }
        if ring.len() < 3 {
            continue;
        }
        let c = ring.iter().fold([0.0; 3], |s, &i| add(s, p(i)));
        vertices.push(scale(c, 1.0 / ring.len() as f64));
        let center = (vertices.len() - 1) as u32;
        for k in 0..ring.len() {
            triangles.push([ring[k], ring[(k + 1) % ring.len()], center]);
        }
    }
    if !merge.is_empty() {
        let resolve = |mut i: u32| {
            while let Some(&j) = merge.get(&i) {
                i = j;
            }
            i
        };
        for t in triangles.iter_mut() {
            *t = t.map(resolve);
        }
        triangles.retain(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0]);
    }
}

/// El agujero no encierra área (respecto de su perímetro) o es tan chico que
/// las caras de un abanico serían degeneradas para OpenCASCADE (< 1 µm).
fn is_flat(ring: &[u32], p: &impl Fn(u32) -> P3) -> bool {
    let o = p(ring[0]);
    let mut area = [0.0; 3];
    let mut perimeter = 0.0;
    for k in 0..ring.len() {
        let (a, b) = (p(ring[k]), p(ring[(k + 1) % ring.len()]));
        area = add(area, cross(sub(a, o), sub(b, o)));
        perimeter += norm(sub(b, a));
    }
    norm(area) < 1e-6 * perimeter * perimeter || perimeter < 1e-3
}

/// Coseno entre los dos lados del vértice `k` del agujero: −1 = en línea recta.
fn straightness(ring: &[u32], k: usize, p: &impl Fn(u32) -> P3) -> f64 {
    let n = ring.len();
    let m = p(ring[k]);
    let (u, v) = (sub(p(ring[(k + n - 1) % n]), m), sub(p(ring[(k + 1) % n]), m));
    dot(u, v) / (norm(u) * norm(v)).max(1e-300)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_a_sliver_hole_by_merging() {
        // Octaedro con una astilla: el vértice 6 en el medio de la arista 0–1
        // parte la cara (0,1,4) en (0,6,4) y (6,1,4); del otro lado la cara
        // (1,0,5) quedó entera: agujero 0→6→1 sin área
        let mut v = vec![
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.5, 0.5, 0.0],
        ];
        let mut t = vec![[0, 6, 4], [6, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4], [1, 0, 5], [2, 1, 5], [3, 2, 5], [0, 3, 5]];
        assert_eq!(solid_problems(&v, &t).unmatched, 3);
        close_open_edges(&mut v, &mut t);
        assert_eq!(solid_problems(&v, &t), SolidProblems::default());
        assert_eq!(t.len(), 8);
    }

    #[test]
    fn closes_a_missing_triangle() {
        // Tetraedro sin una cara: queda cerrado y con la orientación de las demás
        let mut v = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut t = vec![[0, 2, 1], [0, 1, 3], [1, 2, 3]];
        assert_eq!(open_edges(&t), 3);
        drop_specks(&mut t, &v);
        assert_eq!(t.len(), 3, "una sola pieza: no es mota");
        close_open_edges(&mut v, &mut t);
        assert_eq!(open_edges(&t), 0);
        let mut directed = std::collections::HashSet::new();
        for tri in &t {
            for k in 0..3 {
                assert!(directed.insert((tri[k], tri[(k + 1) % 3])), "arista repetida en el mismo sentido");
            }
        }
    }
}
