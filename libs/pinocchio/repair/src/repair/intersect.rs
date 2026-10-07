//! Resolución de auto-intersecciones: corta la malla por las curvas donde se
//! cruzan sus triángulos y conserva solo la cáscara exterior (la unión de los
//! cuerpos solapados).
//!
//! 1. Por cada par de triángulos que se cruzan, el segmento de cruce. Las
//!    decisiones topológicas (qué arista atraviesa qué triángulo) usan
//!    predicados exactos (`robust::orient3d`) sobre claves canónicas, así las
//!    dos caras de una arista ven siempre los mismos puntos de corte.
//! 2. Puntos triples (tres caras que se cortan en un punto): se registran de
//!    forma global para que las tres caras partan sus segmentos igual.
//! 3. Cada cara tocada se retriangula en su espacio paramétrico (u, v) con una
//!    triangulación de Delaunay restringida a sus bordes y segmentos.
//! 4. Las curvas de corte separan la superficie en parches; cada parche se
//!    clasifica con el número de vueltas generalizado (Jacobson et al. 2013)
//!    de la malla original y se descartan los que quedan dentro de otro
//!    cuerpo.
//!
//! Antes de cortar, los vértices se mueven una fracción ínfima (1e-8 de la
//! diagonal) para que los contactos exactos de los modelos CAD (caras
//! coplanares, aristas justo sobre otra cara) pasen a ser cruces genéricos.
//!
//! En superficies abiertas el número de vueltas no separa bien dentro de
//! fuera; si el resultado queda con más bordes o aristas non-manifold que la
//! entrada, se descarta y la malla no cambia ([`IntersectionReport::reverted`]).

use crate::analysis::intersections::candidate_pairs;
use crate::trimesh::TriMesh;
use pinocchio_math::{Real, Vector3};
use robust::{orient2d, orient3d, Coord, Coord3D};
use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2, Triangulation};
use std::collections::{HashMap, HashSet};

/// Resultado de [`resolve_self_intersections`]
#[derive(Debug, Clone, Default)]
pub struct IntersectionReport {
    /// Pares de triángulos cortados por su curva de cruce
    pub pairs_cut: usize,
    /// Pares que se cruzan de forma degenerada y no se cortaron
    pub pairs_skipped: usize,
    /// Parches de superficie descartados por quedar dentro de otro cuerpo
    pub patches_removed: usize,
    /// Triángulos de la malla resultante menos los de la original
    pub face_delta: isize,
    /// Cruces entre segmentos que la triangulación tuvo que partir por su
    /// cuenta (pueden dejar grietas microscópicas)
    pub unexpected_splits: usize,
    /// El resultado quedaba con más bordes o aristas non-manifold que la
    /// entrada (superficies abiertas) y se descartó: la malla no cambió
    pub reverted: bool,
}

impl IntersectionReport {
    /// Si la malla cambió
    pub fn changed(&self) -> bool {
        !self.reverted && (self.pairs_cut > 0 || self.patches_removed > 0)
    }
}

/// Identidad de un vértice nuevo, independiente de qué cara lo crea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Key {
    /// Vértice original (extremo de un cruce entre caras que lo comparten)
    Vertex(usize),
    /// Arista (a < b) que atraviesa la cara
    Edge(usize, usize, usize),
    /// Punto común de tres caras (ordenadas)
    Triple(usize, usize, usize),
}

impl Key {
    fn triple(a: usize, b: usize, c: usize) -> Self {
        let mut f = [a, b, c];
        f.sort_unstable();
        Key::Triple(f[0], f[1], f[2])
    }
}

/// Cruce entre dos caras
enum Crossing {
    None,
    /// Toque o cruce degenerado que no se corta
    Degenerate,
    Segment(Key, Key),
}

/// Segmento de cruce dentro de una cara: extremos y la otra cara del par.
#[derive(Debug, Clone, Copy)]
struct Segment {
    p: Key,
    q: Key,
    partner: usize,
}

/// Corta la malla por sus auto-intersecciones y elimina la superficie que
/// queda dentro de otro cuerpo. Supone caras orientadas hacia afuera.
pub fn resolve_self_intersections(mesh: &mut TriMesh) -> IntersectionReport {
    let mut report = IntersectionReport::default();
    if mesh.num_faces() < 2 {
        return report;
    }
    // Perturbación mínima y determinista: los contactos exactos (caras
    // coplanares, una arista justo sobre otra cara, típicos de CAD) pasan a
    // ser cruces genéricos. Lejos de los cortes, los vértices recuperan su
    // posición al final.
    let mut work = mesh.clone();
    jitter(&mut work, JITTER * mesh.diagonal());
    let mut cut = Cutter::new(&work);

    // 1. Segmentos de cruce
    let pairs = candidate_pairs(&work);
    let crossings: Vec<((usize, usize), Crossing)> = {
        let test = |&(a, b): &(usize, usize)| ((a, b), cut.pair_crossing(a, b));
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            pairs.par_iter().map(test).filter(|(_, c)| !matches!(c, Crossing::None)).collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            pairs.iter().map(test).filter(|(_, c)| !matches!(c, Crossing::None)).collect()
        }
    };
    for ((fa, fb), crossing) in crossings {
        match crossing {
            Crossing::None => {}
            Crossing::Degenerate => report.pairs_skipped += 1,
            Crossing::Segment(p, q) => {
                let (p, q) = (cut.register(p), cut.register(q));
                cut.segments.entry(fa).or_default().push(Segment { p, q, partner: fb });
                cut.segments.entry(fb).or_default().push(Segment { p, q, partner: fa });
                report.pairs_cut += 1;
            }
        }
    }
    if report.pairs_cut == 0 {
        return report;
    }

    // 2. Puntos triples
    let touched: Vec<usize> = {
        let mut set: HashSet<usize> = cut.segments.keys().copied().collect();
        for &(a, b) in cut.edge_points.keys() {
            set.extend(cut.edge_faces(a, b));
        }
        let mut v: Vec<usize> = set.into_iter().collect();
        v.sort_unstable();
        v
    };
    for &f in &touched {
        cut.find_triples(f);
    }

    // 3. Retriangulación de las caras tocadas
    let mut triangles = Vec::with_capacity(work.num_faces() + 8 * touched.len());
    let mut parent = Vec::with_capacity(triangles.capacity());
    let mut cut_edges = HashSet::new();
    let touched_set: HashSet<usize> = touched.iter().copied().collect();
    for f in 0..work.num_faces() {
        if touched_set.contains(&f) {
            let before = triangles.len();
            report.unexpected_splits += cut.retriangulate(f, &mut triangles, &mut cut_edges);
            parent.extend(std::iter::repeat_n(f, triangles.len() - before));
        } else {
            triangles.push(work.triangles[f]);
            parent.push(f);
        }
    }

    // Unificar vértices que la triangulación encontró coincidentes
    let mut alias = cut.alias;
    for t in &mut triangles {
        for v in t.iter_mut() {
            *v = alias.find(*v);
        }
    }
    let cut_edges: HashSet<(usize, usize)> = cut_edges
        .into_iter()
        .map(|(a, b)| {
            let (a, b) = (alias.find(a), alias.find(b));
            (a.min(b), a.max(b))
        })
        .collect();
    let keep: Vec<bool> = triangles.iter().map(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0]).collect();
    let (triangles, parent): (Vec<_>, Vec<_>) =
        triangles.into_iter().zip(parent).zip(keep).filter(|(_, k)| *k).map(|(tp, _)| tp).unzip();

    // 4. Parches y clasificación
    let positions = cut.positions;
    let patches = patches(&triangles, &cut_edges);
    let num_patches = patches.iter().copied().max().map_or(0, |m| m + 1);
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); num_patches];
    for (t, &p) in patches.iter().enumerate() {
        members[p].push(t);
    }
    let area = |t: usize| {
        let [a, b, c] = triangles[t].map(|v| positions[v]);
        (b - a).cross(&(c - a)).length()
    };
    // Parches que tocan una curva de corte
    let mut touches_cut = vec![false; num_patches];
    for (t, tri) in triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            if cut_edges.contains(&(a.min(b), a.max(b))) {
                touches_cut[patches[t]] = true;
            }
        }
    }
    let inside: Vec<bool> = map_patches(&members, |p, faces| {
        let mut by_area: Vec<(Real, usize)> = faces.iter().map(|&t| (area(t), t)).collect();
        by_area.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        let mut w: Vec<Real> = by_area
            .iter()
            .take(3)
            .map(|&(_, t)| {
                let [a, b, c] = triangles[t].map(|v| positions[v]);
                winding_except(&work, (a + b + c) / 3.0, parent[t])
            })
            .collect();
        w.sort_unstable_by(Real::total_cmp);
        let w = w[w.len() / 2];
        // Sin la cara propia, el borde del sólido (0 delante, 1 detrás) da
        // 0,5. Dentro de otro cuerpo: 1,5 o más. Alrededor de un pliegue
        // dado vuelta (región con −1): −0,5. Una lámina abierta da cerca de
        // 0, así que el umbral negativo solo vale junto a un corte.
        w > 1.0 || (touches_cut[p] && w < -0.25)
    });
    report.patches_removed = inside.iter().filter(|&&i| i).count();

    let result: Vec<[usize; 3]> =
        triangles.iter().zip(&patches).filter(|(_, p)| !inside[**p]).map(|(t, _)| *t).collect();

    // Si el resultado tiene más bordes o aristas non-manifold que la entrada
    // (superficies abiertas, donde dentro y fuera no están bien definidos),
    // se descarta entero
    if open_or_non_manifold_edges(&result) > open_or_non_manifold_edges(&mesh.triangles) {
        report.reverted = true;
        return report;
    }
    report.face_delta = result.len() as isize - mesh.num_faces() as isize;
    // Lejos de los cortes, las posiciones originales exactas; en las caras
    // cortadas se conserva la perturbación para que los cortes sigan
    // coincidiendo
    let mut positions = positions;
    let mut moved = vec![false; mesh.num_vertices()];
    for &f in &touched {
        for &v in &mesh.triangles[f] {
            moved[v] = true;
        }
    }
    for (v, p) in mesh.positions.iter().enumerate() {
        if !moved[v] {
            positions[v] = *p;
        }
    }
    *mesh = TriMesh::new(positions, result);
    mesh.remove_unreferenced_vertices();
    report
}

/// Perturbación relativa a la diagonal (ver [`resolve_self_intersections`])
const JITTER: Real = 1e-8;

/// Mueve cada vértice hasta `amount` en cada eje, de forma determinista.
fn jitter(mesh: &mut TriMesh, amount: Real) {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = || {
        // splitmix64
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as Real / (1u64 << 53) as Real * 2.0 - 1.0
    };
    for p in &mut mesh.positions {
        *p += Vector3::new(next(), next(), next()) * amount;
    }
}

/// Aristas con una sola cara o con más de dos.
fn open_or_non_manifold_edges(triangles: &[[usize; 3]]) -> usize {
    let mut count: HashMap<(usize, usize), u32> = HashMap::with_capacity(triangles.len() * 3 / 2);
    for t in triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    count.values().filter(|&&c| c != 2).count()
}

/// Aplica `f` a cada parche (en paralelo con la característica `parallel`).
fn map_patches(members: &[Vec<usize>], f: impl Fn(usize, &[usize]) -> bool + Sync) -> Vec<bool> {
    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;
        members.par_iter().enumerate().map(|(p, m)| f(p, m)).collect()
    }
    #[cfg(not(feature = "parallel"))]
    {
        members.iter().enumerate().map(|(p, m)| f(p, m)).collect()
    }
}

/// Número de vueltas generalizado de la malla en `p`, sin la cara `skip`
/// (sobre la que está `p`). Para una cáscara cerrada orientada hacia afuera
/// vale 0 fuera y 1 dentro; la cara propia aporta ∓½ según el lado.
fn winding_except(mesh: &TriMesh, p: Vector3, skip: usize) -> Real {
    let mut sum = 0.0;
    for (f, t) in mesh.triangles.iter().enumerate() {
        if f == skip {
            continue;
        }
        let [a, b, c] = t.map(|v| mesh.positions[v] - p);
        let (la, lb, lc) = (a.length(), b.length(), c.length());
        let det = a.dot(&b.cross(&c));
        let den = la * lb * lc + a.dot(&b) * lc + b.dot(&c) * la + c.dot(&a) * lb;
        sum += 2.0 * det.atan2(den);
    }
    sum / (4.0 * std::f64::consts::PI)
}

/// Componentes conexas por aristas manifold que no son curvas de corte.
fn patches(triangles: &[[usize; 3]], cut_edges: &HashSet<(usize, usize)>) -> Vec<usize> {
    let mut edge_faces: HashMap<(usize, usize), Vec<usize>> = HashMap::with_capacity(triangles.len() * 3 / 2);
    for (t, tri) in triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            edge_faces.entry((a.min(b), a.max(b))).or_default().push(t);
        }
    }
    let mut patch = vec![usize::MAX; triangles.len()];
    let mut next = 0;
    let mut stack = Vec::new();
    for seed in 0..triangles.len() {
        if patch[seed] != usize::MAX {
            continue;
        }
        patch[seed] = next;
        stack.push(seed);
        while let Some(t) = stack.pop() {
            let tri = triangles[t];
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                let e = (a.min(b), a.max(b));
                if cut_edges.contains(&e) {
                    continue;
                }
                let faces = &edge_faces[&e];
                if faces.len() != 2 {
                    continue;
                }
                let n = if faces[0] == t { faces[1] } else { faces[0] };
                if patch[n] == usize::MAX {
                    patch[n] = next;
                    stack.push(n);
                }
            }
        }
        next += 1;
    }
    patch
}

/// Unión-búsqueda para vértices que resultan coincidentes.
#[derive(Default)]
struct Alias(Vec<usize>);

impl Alias {
    fn ensure(&mut self, n: usize) {
        while self.0.len() < n {
            let i = self.0.len();
            self.0.push(i);
        }
    }

    fn find(&mut self, v: usize) -> usize {
        self.ensure(v + 1);
        let mut r = v;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut c = v;
        while self.0[c] != r {
            let n = self.0[c];
            self.0[c] = r;
            c = n;
        }
        r
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Conserva el índice menor (los vértices originales primero)
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.0[hi] = lo;
        }
    }
}

struct Cutter<'a> {
    mesh: &'a TriMesh,
    positions: Vec<Vector3>,
    ids: HashMap<Key, usize>,
    /// Parámetro t ∈ [0, 1] de los puntos sobre aristas (de a hacia b)
    edge_t: HashMap<Key, Real>,
    /// Puntos de corte por arista (a < b)
    edge_points: HashMap<(usize, usize), Vec<Key>>,
    /// Segmentos de cruce por cara
    segments: HashMap<usize, Vec<Segment>>,
    /// Puntos triples por par de caras (ordenado)
    triples: HashMap<(usize, usize), Vec<Key>>,
    triple_set: HashSet<Key>,
    /// Caras por arista de la malla original
    edges: HashMap<(usize, usize), Vec<usize>>,
    alias: Alias,
}

fn c3(v: Vector3) -> Coord3D<f64> {
    Coord3D { x: v.x(), y: v.y(), z: v.z() }
}

fn c2(p: Point2<f64>) -> Coord<f64> {
    Coord { x: p.x, y: p.y }
}

/// Esquinas de la cara en su espacio paramétrico
const CORNERS: [[f64; 2]; 3] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
/// Aristas de borde (bits) que contienen cada esquina: la esquina k está en
/// las aristas k y k − 1.
const CORNER_MASK: [u8; 3] = [0b101, 0b011, 0b110];

#[derive(Clone, Copy)]
struct PVert {
    p: Point2<f64>,
}

impl HasPosition for PVert {
    type Scalar = f64;
    fn position(&self) -> Point2<f64> {
        self.p
    }
}

impl<'a> Cutter<'a> {
    fn new(mesh: &'a TriMesh) -> Self {
        let mut edges: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (f, t) in mesh.triangles.iter().enumerate() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                edges.entry((a.min(b), a.max(b))).or_default().push(f);
            }
        }
        Self {
            mesh,
            positions: mesh.positions.clone(),
            ids: HashMap::new(),
            edge_t: HashMap::new(),
            edge_points: HashMap::new(),
            segments: HashMap::new(),
            triples: HashMap::new(),
            triple_set: HashSet::new(),
            edges,
            alias: Alias::default(),
        }
    }

    fn edge_faces(&self, a: usize, b: usize) -> Vec<usize> {
        self.edges.get(&(a, b)).cloned().unwrap_or_default()
    }

    /// Signo del lado de `p` respecto del plano de la cara (el cero cuenta
    /// como positivo: perturbación simbólica consistente).
    fn side(&self, f: usize, p: Vector3) -> (bool, f64) {
        let [a, b, c] = self.mesh.corners(f);
        let o = orient3d(c3(a), c3(b), c3(c), c3(p));
        (o >= 0.0, o)
    }

    /// Si la arista (a, b) atraviesa la cara `f`.
    fn crosses(&self, a: usize, b: usize, f: usize) -> bool {
        let (pa, pb) = (self.mesh.positions[a], self.mesh.positions[b]);
        if self.side(f, pa).0 == self.side(f, pb).0 {
            return false;
        }
        let [p, q, r] = self.mesh.corners(f);
        let s1 = orient3d(c3(pa), c3(pb), c3(p), c3(q)) >= 0.0;
        let s2 = orient3d(c3(pa), c3(pb), c3(q), c3(r)) >= 0.0;
        let s3 = orient3d(c3(pa), c3(pb), c3(r), c3(p)) >= 0.0;
        s1 == s2 && s2 == s3
    }

    /// Registra un extremo de segmento y devuelve su clave.
    fn register(&mut self, key: Key) -> Key {
        if self.ids.contains_key(&key) {
            return key;
        }
        match key {
            Key::Vertex(v) => {
                self.ids.insert(key, v);
            }
            Key::Edge(a, b, f) => {
                let (pa, pb) = (self.mesh.positions[a], self.mesh.positions[b]);
                let (da, db) = (self.side(f, pa).1, self.side(f, pb).1);
                let t = if da == db { 0.5 } else { (da / (da - db)).clamp(0.0, 1.0) };
                self.ids.insert(key, self.positions.len());
                self.positions.push(pa + (pb - pa) * t);
                self.edge_t.insert(key, t);
                self.edge_points.entry((a, b)).or_default().push(key);
            }
            Key::Triple(..) => unreachable!("los puntos triples se registran aparte"),
        }
        key
    }

    /// Si los vértices `vs` de una cara quedan estrictamente del mismo lado
    /// del plano de `f` (descarte rápido).
    fn same_side(&self, f: usize, vs: impl Iterator<Item = usize>) -> bool {
        let mut sign = 0.0;
        for v in vs {
            let o = self.side(f, self.mesh.positions[v]).1;
            if o == 0.0 || (sign != 0.0 && (o > 0.0) != (sign > 0.0)) {
                return false;
            }
            sign = o;
        }
        true
    }

    /// Segmento de cruce entre dos caras (sin modificar nada).
    fn pair_crossing(&self, fa: usize, fb: usize) -> Crossing {
        let (ta, tb) = (self.mesh.triangles[fa], self.mesh.triangles[fb]);
        let shared: Vec<usize> = ta.iter().copied().filter(|v| tb.contains(v)).collect();
        let free = |t: [usize; 3]| t.into_iter().filter(|v| !shared.contains(v));
        if self.same_side(fb, free(ta)) || self.same_side(fa, free(tb)) {
            return Crossing::None;
        }
        let mut found = Vec::with_capacity(2);
        for (f, g) in [(fa, fb), (fb, fa)] {
            let t = self.mesh.triangles[f];
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                // Con un vértice compartido solo cuenta la arista opuesta
                if shared.contains(&a) || shared.contains(&b) {
                    continue;
                }
                let (a, b) = (a.min(b), a.max(b));
                if self.crosses(a, b, g) {
                    found.push(Key::Edge(a, b, g));
                }
            }
        }
        match (shared.len(), found.len()) {
            (_, 0) => Crossing::None,
            (0, 2) => Crossing::Segment(found[0], found[1]),
            (1, 1) => Crossing::Segment(Key::Vertex(shared[0]), found[0]),
            _ => Crossing::Degenerate,
        }
    }

    /// Coordenadas (u, v) de un punto 3D en el espacio paramétrico de `f`.
    fn project(&self, f: usize, p: Vector3) -> [f64; 2] {
        let [a, b, c] = self.mesh.corners(f);
        let (e1, e2, d) = (b - a, c - a, p - a);
        let (g11, g12, g22) = (e1.dot(&e1), e1.dot(&e2), e2.dot(&e2));
        let (r1, r2) = (e1.dot(&d), e2.dot(&d));
        let det = g11 * g22 - g12 * g12;
        if det.abs() < f64::MIN_POSITIVE {
            return [0.0, 0.0];
        }
        [(r1 * g22 - r2 * g12) / det, (r2 * g11 - r1 * g12) / det]
    }

    fn unproject(&self, f: usize, uv: [f64; 2]) -> Vector3 {
        let [a, b, c] = self.mesh.corners(f);
        a + (b - a) * uv[0] + (c - a) * uv[1]
    }

    /// Posición paramétrica de un punto en la cara `f` y su máscara de
    /// aristas de borde.
    fn param(&self, f: usize, key: Key) -> ([f64; 2], u8) {
        if let Key::Vertex(v) = key {
            if let Some(k) = self.mesh.triangles[f].iter().position(|&x| x == v) {
                return (CORNERS[k], CORNER_MASK[k]);
            }
        }
        if let Key::Edge(a, b, _) = key {
            let t = self.mesh.triangles[f];
            for k in 0..3 {
                let (i, j) = (t[k], t[(k + 1) % 3]);
                if (i, j) == (a, b) || (j, i) == (a, b) {
                    let mut s = self.edge_t[&key];
                    if i != a {
                        s = 1.0 - s;
                    }
                    let (p, q) = (CORNERS[k], CORNERS[(k + 1) % 3]);
                    // Sobre la arista exactamente: la hipotenusa como (1 − s, s)
                    let uv = match k {
                        0 => [s, 0.0],
                        1 => [1.0 - s, s],
                        _ => [0.0, 1.0 - s],
                    };
                    debug_assert!((uv[0] - (p[0] + s * (q[0] - p[0]))).abs() < 1e-12);
                    return (uv, 1 << k);
                }
            }
        }
        (self.project(f, self.positions[self.ids[&key]]), 0)
    }

    /// Busca cruces entre los segmentos de la cara `f` y los registra como
    /// puntos triples.
    fn find_triples(&mut self, f: usize) {
        let Some(segs) = self.segments.get(&f) else { return };
        if segs.len() < 2 {
            return;
        }
        let ends: Vec<([f64; 2], [f64; 2])> = segs.iter().map(|s| (self.param(f, s.p).0, self.param(f, s.q).0)).collect();
        let mut found = Vec::new();
        for i in 0..segs.len() {
            for j in i + 1..segs.len() {
                let (s, t) = (segs[i], segs[j]);
                if s.partner == t.partner || s.p == t.p || s.p == t.q || s.q == t.p || s.q == t.q {
                    continue;
                }
                let pt = |x: [f64; 2]| Coord { x: x[0], y: x[1] };
                let (a, b) = (pt(ends[i].0), pt(ends[i].1));
                let (c, d) = (pt(ends[j].0), pt(ends[j].1));
                let (o1, o2) = (orient2d(a, b, c), orient2d(a, b, d));
                let (o3, o4) = (orient2d(c, d, a), orient2d(c, d, b));
                if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
                    // Cruce en 2D (fallback si los tres planos son casi paralelos)
                    let s_ = o3 / (o3 - o4);
                    let uv = [ends[i].0[0] + s_ * (ends[i].1[0] - ends[i].0[0]), ends[i].0[1] + s_ * (ends[i].1[1] - ends[i].0[1])];
                    found.push((Key::triple(f, s.partner, t.partner), self.unproject(f, uv)));
                }
            }
        }
        for (key, fallback) in found {
            if self.triple_set.insert(key) {
                let Key::Triple(a, b, c) = key else { unreachable!() };
                let p = self.three_planes(a, b, c).unwrap_or(fallback);
                self.ids.insert(key, self.positions.len());
                self.positions.push(p);
                for pair in [(a, b), (a, c), (b, c)] {
                    self.triples.entry(pair).or_default().push(key);
                }
            }
        }
    }

    /// Intersección de los planos de tres caras.
    fn three_planes(&self, a: usize, b: usize, c: usize) -> Option<Vector3> {
        let plane = |f: usize| {
            let n = self.mesh.face_cross(f).try_normalize()?;
            Some((n, n.dot(&self.mesh.positions[self.mesh.triangles[f][0]])))
        };
        let ((n1, d1), (n2, d2), (n3, d3)) = (plane(a)?, plane(b)?, plane(c)?);
        let det = n1.dot(&n2.cross(&n3));
        if det.abs() < 1e-6 {
            return None;
        }
        Some((n2.cross(&n3) * d1 + n3.cross(&n1) * d2 + n1.cross(&n2) * d3) / det)
    }

    /// Retriangula la cara `f` con sus puntos y segmentos de corte. Devuelve
    /// cuántos vértices tuvo que crear la triangulación por su cuenta.
    fn retriangulate(&mut self, f: usize, out: &mut Vec<[usize; 3]>, cut_edges: &mut HashSet<(usize, usize)>) -> usize {
        let tri = self.mesh.triangles[f];
        let mut cdt: ConstrainedDelaunayTriangulation<PVert> = ConstrainedDelaunayTriangulation::new();
        // Por vértice de la triangulación: índice global y aristas de borde
        let mut ids: Vec<usize> = Vec::new();
        let mut masks: Vec<u8> = Vec::new();
        let mut handle_of: HashMap<Key, spade::handles::FixedVertexHandle> = HashMap::new();

        let mut insert = |cdt: &mut ConstrainedDelaunayTriangulation<PVert>,
                          alias: &mut Alias,
                          uv: [f64; 2],
                          id: usize,
                          mask: u8| {
            let h = cdt.insert(PVert { p: Point2::new(uv[0], uv[1]) }).expect("coordenadas paramétricas finitas");
            if h.index() < ids.len() {
                alias.union(ids[h.index()], id);
                masks[h.index()] |= mask;
            } else {
                ids.push(id);
                masks.push(mask);
            }
            h
        };

        let corner_h: Vec<_> =
            (0..3).map(|k| insert(&mut cdt, &mut self.alias, CORNERS[k], tri[k], CORNER_MASK[k])).collect();

        // Puntos sobre las aristas de la cara, ordenados de la esquina k a la k+1
        let mut boundary: Vec<Vec<(f64, spade::handles::FixedVertexHandle)>> = vec![Vec::new(); 3];
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            let Some(keys) = self.edge_points.get(&(a.min(b), a.max(b))) else { continue };
            for &key in keys {
                let (uv, mask) = self.param(f, key);
                let h = insert(&mut cdt, &mut self.alias, uv, self.ids[&key], mask);
                handle_of.insert(key, h);
                let s = match k {
                    0 => uv[0],
                    1 => uv[1],
                    _ => 1.0 - uv[1],
                };
                boundary[k].push((s, h));
            }
        }

        // Extremos de segmentos y puntos triples
        let segs = self.segments.get(&f).cloned().unwrap_or_default();
        let mut chains: Vec<Vec<Key>> = Vec::with_capacity(segs.len());
        for s in &segs {
            let mut chain = vec![s.p, s.q];
            if let Some(tr) = self.triples.get(&(f.min(s.partner), f.max(s.partner))) {
                chain.extend(tr.iter().copied());
            }
            for &key in &chain {
                if !handle_of.contains_key(&key) {
                    let (uv, mask) = self.param(f, key);
                    let h = insert(&mut cdt, &mut self.alias, uv, self.ids[&key], mask);
                    handle_of.insert(key, h);
                }
            }
            // Ordenar a lo largo del segmento
            let p0 = cdt.vertex(handle_of[&s.p]).position();
            let p1 = cdt.vertex(handle_of[&s.q]).position();
            let dir = [p1.x - p0.x, p1.y - p0.y];
            let along = |k: &Key| {
                let p = cdt.vertex(handle_of[k]).position();
                (p.x - p0.x) * dir[0] + (p.y - p0.y) * dir[1]
            };
            chain.sort_by(|a, b| along(a).total_cmp(&along(b)));
            chain.dedup();
            chains.push(chain);
        }

        let constrain = |cdt: &mut ConstrainedDelaunayTriangulation<PVert>, a, b| {
            if a != b {
                cdt.add_constraint_and_split(a, b, |p| PVert { p });
            }
        };
        for k in 0..3 {
            let mut chain = std::mem::take(&mut boundary[k]);
            chain.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut prev = corner_h[k];
            for (_, h) in chain {
                constrain(&mut cdt, prev, h);
                prev = h;
            }
            constrain(&mut cdt, prev, corner_h[(k + 1) % 3]);
        }
        for chain in &chains {
            for w in chain.windows(2) {
                constrain(&mut cdt, handle_of[&w[0]], handle_of[&w[1]]);
            }
        }

        // Vértices que creó la triangulación al partir cruces imprevistos
        let mut unexpected = 0;
        for h in cdt.fixed_vertices() {
            if h.index() >= ids.len() {
                let p = cdt.vertex(h).position();
                ids.push(self.positions.len());
                masks.push(0);
                self.positions.push(self.unproject(f, [p.x, p.y]));
                unexpected += 1;
            }
        }

        for face in cdt.inner_faces() {
            let v = face.vertices();
            let hs = v.map(|x| x.fix().index());
            if masks[hs[0]] & masks[hs[1]] & masks[hs[2]] != 0 {
                continue; // astilla sobre una arista de borde
            }
            let [a, b, c] = v.map(|x| c2(x.position()));
            if orient2d(a, b, c) == 0.0 {
                continue;
            }
            out.push(hs.map(|h| ids[h]));
        }
        for e in cdt.undirected_edges() {
            if e.is_constraint_edge() {
                let [a, b] = e.vertices().map(|x| x.fix().index());
                if masks[a] & masks[b] == 0 {
                    cut_edges.insert((ids[a], ids[b]));
                }
            }
        }
        unexpected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze_trimesh;
    use crate::config::AnalysisConfig;

    /// Caja cerrada [min, max] con normales hacia afuera, añadida a `mesh`.
    fn add_box(mesh: &mut TriMesh, min: [f64; 3], max: [f64; 3]) {
        let base = mesh.positions.len();
        for i in 0..8 {
            mesh.positions.push(Vector3::new(
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ));
        }
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        for q in quads {
            mesh.triangles.push([base + q[0], base + q[1], base + q[2]]);
            mesh.triangles.push([base + q[0], base + q[2], base + q[3]]);
        }
    }

    fn health(mesh: &TriMesh) -> crate::config::MeshDiagnostics {
        analyze_trimesh(mesh, &AnalysisConfig { check_self_intersections: true, ..Default::default() })
    }

    #[test]
    fn box_is_outward() {
        let mut m = TriMesh::default();
        add_box(&mut m, [0.0; 3], [1.0; 3]);
        assert!((m.signed_volume() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn two_overlapping_boxes_become_their_union() {
        let mut m = TriMesh::default();
        add_box(&mut m, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        add_box(&mut m, [1.0, 0.7, 0.6], [3.0, 2.6, 2.3]);
        assert!(health(&m).self_intersections > 0);

        let report = resolve_self_intersections(&mut m);
        assert!(report.changed(), "{report:?}");
        assert_eq!(report.pairs_skipped, 0, "{report:?}");
        let d = health(&m);
        assert!(d.is_closed && d.is_manifold, "{d:#?}\n{report:?}");
        assert_eq!(d.self_intersections, 0, "{d:#?}");
        // 8 + 2·1,9·1,7 − 1·1,3·1,4
        let expected = 8.0 + 2.0 * 1.9 * 1.7 - 1.0 * 1.3 * 1.4;
        assert!((m.signed_volume() - expected).abs() < 1e-6, "{} vs {expected}", m.signed_volume());
    }

    #[test]
    fn box_inside_box_is_removed() {
        let mut m = TriMesh::default();
        add_box(&mut m, [0.0; 3], [3.0; 3]);
        add_box(&mut m, [1.0; 3], [2.0; 3]);
        // Esquina de la tercera fuera de las diagonales de las caras (un cruce
        // justo sobre una diagonal es degenerado y no se corta)
        add_box(&mut m, [2.5, 2.6, 2.7], [4.0, 4.1, 4.2]);
        let report = resolve_self_intersections(&mut m);
        assert!(report.changed());
        assert_eq!(report.pairs_skipped, 0, "{report:?}");
        let d = health(&m);
        assert!(d.is_closed && d.is_manifold, "{d:#?}");
        assert_eq!(d.self_intersections, 0);
        let expected = 27.0 + 1.5 * 1.5 * 1.5 - 0.5 * 0.4 * 0.3;
        assert!((m.signed_volume() - expected).abs() < 1e-6, "{} vs {expected}", m.signed_volume());
    }

    #[test]
    fn three_boxes_with_triple_points() {
        let mut m = TriMesh::default();
        add_box(&mut m, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        add_box(&mut m, [1.0, 0.3, 0.4], [3.0, 1.6, 1.7]);
        add_box(&mut m, [0.5, 1.1, 0.9], [1.8, 2.9, 2.6]);
        let report = resolve_self_intersections(&mut m);
        assert!(report.changed());
        let d = health(&m);
        assert!(d.is_closed && d.is_manifold, "{d:#?}\n{report:?}");
        assert_eq!(d.self_intersections, 0);
    }

    /// Esfera UV cerrada (polos compartidos) con normales hacia afuera.
    fn add_sphere(mesh: &mut TriMesh, center: [f64; 3], r: f64, rings: usize, segments: usize) {
        let base = mesh.positions.len();
        let at = |v: Vector3| Vector3::new(center[0], center[1], center[2]) + v * r;
        mesh.positions.push(at(Vector3::new(0.0, 0.0, 1.0)));
        for i in 1..rings {
            let th = std::f64::consts::PI * i as f64 / rings as f64;
            for j in 0..segments {
                let ph = 2.0 * std::f64::consts::PI * j as f64 / segments as f64;
                mesh.positions.push(at(Vector3::new(th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos())));
            }
        }
        mesh.positions.push(at(Vector3::new(0.0, 0.0, -1.0)));
        let ring = |i: usize, j: usize| base + 1 + (i - 1) * segments + j % segments;
        let south = base + 1 + (rings - 1) * segments;
        for j in 0..segments {
            mesh.triangles.push([base, ring(1, j), ring(1, j + 1)]);
            mesh.triangles.push([south, ring(rings - 1, j + 1), ring(rings - 1, j)]);
            for i in 1..rings - 1 {
                mesh.triangles.push([ring(i, j), ring(i + 1, j), ring(i + 1, j + 1)]);
                mesh.triangles.push([ring(i, j), ring(i + 1, j + 1), ring(i, j + 1)]);
            }
        }
    }

    #[test]
    fn overlapping_spheres() {
        let mut m = TriMesh::default();
        add_sphere(&mut m, [0.0, 0.0, 0.0], 1.0, 24, 32);
        add_sphere(&mut m, [0.9, 0.3, 0.2], 0.8, 20, 28);
        let v = m.signed_volume();
        assert!(v > 0.0);
        let report = resolve_self_intersections(&mut m);
        assert!(report.changed() && report.pairs_skipped == 0, "{report:?}");
        let d = health(&m);
        assert!(d.is_closed && d.is_manifold, "{d:#?}\n{report:?}");
        assert_eq!(d.self_intersections, 0);
        assert_eq!(d.connected_components, 1);
        // La unión es menor que la suma y mayor que la esfera grande (≈ 4,1)
        assert!(m.signed_volume() < v - 0.1 && m.signed_volume() > 4.1, "{} {v}", m.signed_volume());
    }

    #[test]
    fn untouched_mesh_is_unchanged() {
        let mut m = TriMesh::default();
        add_box(&mut m, [0.0; 3], [1.0; 3]);
        add_box(&mut m, [2.0; 3], [3.0; 3]);
        let before = m.clone();
        let report = resolve_self_intersections(&mut m);
        assert!(!report.changed());
        assert_eq!(m.triangles, before.triangles);
    }
}
