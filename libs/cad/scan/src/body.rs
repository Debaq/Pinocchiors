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
use quadriflow_core::remesh::{IsotropicOptions, VoxelOptions, isotropic, surface_stats, voxel, voxel_grid};

/// Cómo preparar el cuerpo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyOptions {
    /// Triángulos aproximados del cuerpo (más: más fiel y booleanas más lentas,
    /// ~0,2 ms por cara).
    pub faces: usize,
    /// Holgura (mm): positiva agranda el escaneo (encajes que no aprieten),
    /// negativa lo achica.
    pub offset: f64,
    /// Distinto de cero: una cáscara de este espesor desde la piel corrida
    /// `offset` (positivo: hacia afuera; negativo: hacia adentro), en vez del
    /// cuerpo lleno.
    pub thickness: f64,
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

/// Vóxeles que tiene que abarcar el espesor de una cáscara (con menos, las
/// dos pieles se tocan y se abren agujeros).
const MIN_SHELL_VOXELS: f64 = 2.5;

/// Triángulos del isótropo / triángulos equiláteros del lado pedido en la misma área.
const ISOTROPIC_FILL: f64 = 0.75;

/// Prepara el escaneo como cuerpo.
pub fn prepare_body(vertices: &[P3], triangles: &[[u32; 3]], options: &BodyOptions) -> Result<BodyMesh, String> {
    if !(options.offset.is_finite() && options.thickness.is_finite()) {
        return Err("la holgura o el espesor no es un número".into());
    }
    let faces = options.faces.clamp(MIN_FACES, MAX_FACES);
    let positions: Vec<[f32; 3]> = vertices.iter().map(|p| p.map(|c| c as f32)).collect();
    let indices: Vec<u32> = triangles.iter().flatten().copied().collect();
    let stats = surface_stats(&positions, &indices);
    if !(stats.area > 0.0) {
        return Err("el escaneo no tiene superficie".into());
    }
    let triangle_area = |edge: f64| 3f64.sqrt() / 4.0 * edge * edge;
    // Vóxeles a la mitad del lado que tendría el cuerpo con el área del
    // escaneo; en una cáscara, también menos de un tercio del espesor (si no,
    // las dos pieles se tocan)
    let rough = (stats.area / triangle_area(1.0) / faces as f64).sqrt();
    let rough = if options.thickness != 0.0 { rough.min(options.thickness.abs() * 2.0 / 3.0) } else { rough };
    if options.thickness != 0.0 {
        // Los triángulos no pueden ser más largos que el espesor (ver abajo):
        // con las dos pieles (~el doble del área), cuántos harían falta
        let needed = |t: f64| ISOTROPIC_FILL * 2.0 * stats.area / triangle_area(t);
        if needed(options.thickness.abs()) > MAX_FACES as f64 {
            let min = (ISOTROPIC_FILL * 2.0 * stats.area / (triangle_area(1.0) * MAX_FACES as f64)).sqrt();
            return Err(format!(
                "la carcasa necesita demasiados triángulos para el tamaño del escaneo ({:.0} mm de largo): espesor de al menos {min:.1} mm",
                stats.extent
            ));
        }
    }
    // La grilla tiene un tope de vóxeles por eje: en escaneos grandes una
    // cáscara fina no entra (sale en pedazos)
    let voxel_size = voxel_grid(&positions, &indices, rough / 2.0).voxel_size;
    if options.thickness != 0.0 && options.thickness.abs() < MIN_SHELL_VOXELS * voxel_size {
        return Err(format!(
            "el espesor es muy fino para el tamaño del escaneo ({:.0} mm de largo): al menos {:.1} mm",
            stats.extent,
            MIN_SHELL_VOXELS * voxel_size
        ));
    }
    // Carcasa de un escaneo abierto: primero cerrado (los huecos tapados).
    // Sobre la malla abierta el signo de la distancia salta en los huecos y
    // la piel de adentro rodea el borde como un tubo del grosor de la
    // holgura, que el isótropo pliega (las pieles se cruzan)
    let reclosed;
    let (positions, indices) = if options.thickness != 0.0 && open_edges(triangles) > 0 {
        // Con vóxeles del doble: la carcasa cuesta según los triángulos que
        // recibe (cada uno recorre la banda), y los de la grilla fina son muchos
        let close = VoxelOptions { voxel_size: rough, smooth_iterations: 0, isotropic_edge: None, offset: 0.0, thickness: 0.0 };
        reclosed = voxel(&positions, &indices, &close).map_err(|e| e.to_string())?;
        (&reclosed.positions, &reclosed.indices)
    } else {
        (&positions, &indices)
    };
    let opts = VoxelOptions { voxel_size: rough / 2.0, smooth_iterations: 0, isotropic_edge: None, offset: options.offset, thickness: options.thickness };
    let closed = voxel(positions, indices, &opts).map_err(|e| match e {
        quadriflow_core::RemeshError::EmptyMesh if options.thickness != 0.0 => "la cáscara quedó vacía".to_string(),
        quadriflow_core::RemeshError::EmptyMesh => "la holgura se comió el escaneo entero".to_string(),
        e => e.to_string(),
    })?;
    // El lado, con el área ya cerrada y corrida (los huecos tapados, el ruido
    // y la holgura la cambian): así salen los triángulos pedidos. El isótropo
    // deja aristas algo más largas que las pedidas: ~¾ de los triángulos
    // equiláteros que cabrían (medido)
    let area = surface_stats(&closed.positions, &closed.indices).area;
    let edge = (ISOTROPIC_FILL * area / triangle_area(1.0) / faces as f64).sqrt();
    // En una carcasa, triángulos más largos que el espesor cruzan las dos
    // pieles en las concavidades: hacen falta más triángulos que los pedidos
    let edge = if options.thickness != 0.0 { edge.min(options.thickness.abs()) } else { edge };
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
        fix_slivers(&vertices, &mut triangles, edge * SLIVER);
        last = solid_problems(&vertices, &triangles);
        if last == SolidProblems::default() {
            return Ok(BodyMesh { vertices, triangles, edge });
        }
    }
    Err(format!("el escaneo no quedó cerrado ({last:?})"))
}

/// Altura mínima de un triángulo, en lados del isótropo.
const SLIVER: f64 = 0.01;

/// Triángulos casi sin altura (lo que a veces dejan el isótropo o el tapado
/// de agujeros): OpenCASCADE los toma con tolerancias que se pisan y las
/// booleanas dan vacío sin avisar. Un capuchón (el vértice casi sobre el lado
/// largo) se arregla girando ese lado con el triángulo vecino; una aguja (un
/// lado casi nulo), juntando sus dos puntas.
fn fix_slivers(vertices: &[P3], triangles: &mut Vec<[u32; 3]>, min_height: f64) {
    let p = |i: u32| vertices[i as usize];
    for _ in 0..4 {
        let mut owner: HashMap<(u32, u32), usize> = HashMap::new();
        for (i, t) in triangles.iter().enumerate() {
            for k in 0..3 {
                owner.insert((t[k], t[(k + 1) % 3]), i);
            }
        }
        let mut ring: HashMap<u32, Vec<u32>> = HashMap::new();
        for t in triangles.iter() {
            for k in 0..3 {
                ring.entry(t[k]).or_default().push(t[(k + 1) % 3]);
            }
        }
        let mut merge: HashMap<u32, u32> = HashMap::new();
        // Vértices cuyo entorno ya cambió en esta pasada
        let mut locked: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut touched = vec![false; triangles.len()];
        let mut changed = false;
        for i in 0..triangles.len() {
            if touched[i] {
                continue;
            }
            let t = triangles[i];
            let len = |k: usize| norm(sub(p(t[(k + 1) % 3]), p(t[k])));
            let k = (0..3).max_by(|&a, &b| len(a).total_cmp(&len(b))).unwrap();
            let area2 = norm(cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0]))));
            if area2 / len(k).max(1e-300) >= min_height {
                continue;
            }
            let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            let shortest = (0..3).map(len).fold(f64::INFINITY, f64::min);
            if shortest < min_height {
                // Aguja: las puntas del lado corto a su punto medio
                let s = (0..3).min_by(|&x, &y| len(x).total_cmp(&len(y))).unwrap();
                let (u, v) = (t[s], t[(s + 1) % 3]);
                if locked.contains(&u) || locked.contains(&v) {
                    continue;
                }
                // Condición de enlace: u y v comparten solo los dos vértices
                // opuestos de sus caras (si no, juntar pellizca la superficie)
                let (nu, nv) = (&ring[&u], &ring[&v]);
                let common = nu.iter().filter(|w| nv.contains(w)).count();
                if common != 2 || nu.len() <= 3 || nv.len() <= 3 {
                    continue;
                }
                locked.extend(nu.iter().chain(nv).copied());
                locked.extend([u, v]);
                merge.insert(v, u);
                touched[i] = true;
                changed = true;
                continue;
            }
            // Capuchón: girar el lado largo a–b con el vecino (b, a, d)
            let Some(&j) = owner.get(&(b, a)) else { continue };
            if touched[j] {
                continue;
            }
            let o = triangles[j];
            let d = o[(o.iter().position(|&x| x == b).unwrap() + 2) % 3];
            if d == c || owner.contains_key(&(c, d)) || owner.contains_key(&(d, c)) || [a, b, c, d].iter().any(|x| locked.contains(x)) {
                continue;
            }
            // Las aristas cerca ya no son las del mapa: nada más en esta pasada
            locked.extend([a, b, c, d]);
            triangles[i] = [a, d, c];
            triangles[j] = [d, b, c];
            touched[i] = true;
            touched[j] = true;
            changed = true;
        }
        if !merge.is_empty() {
            // La punta que se va queda en el lugar de la otra (sin mover
            // vértices: los demás triángulos que la usan siguen igual de planos)
            for t in triangles.iter_mut() {
                *t = t.map(|i| merge.get(&i).copied().unwrap_or(i));
            }
            triangles.retain(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0]);
        }
        if !changed {
            break;
        }
    }
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
/// Los huecos (pieles hacia adentro de una cáscara) cuentan por su tamaño.
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
    let largest = volume.values().map(|v| v.abs()).fold(0.0, f64::max);
    let mut k = 0;
    triangles.retain(|_| {
        k += 1;
        volume[&roots[k - 1]].abs() >= largest * 0.01
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
