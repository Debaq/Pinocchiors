//! Booleanas con mallas grandes (libs/cad/PLAN_MALLAS.md).
//!
//! Una pieza que nace de una malla (el escaneo como cuerpo) es, para OCCT, un
//! sólido de una cara por triángulo: con 100 000+ caras cada booleana tardaba
//! minutos (un grabado de texto sobre un hueso de 163 000 triángulos, 29 min).
//! Aquí la booleana se hace sobre triángulos con manifold-rust (port de
//! Manifold, el motor de OpenSCAD y Blender): el mismo grabado, ~1,5 s. La
//! herramienta OCCT entra teselada.

use std::collections::HashMap;
use std::sync::Arc;

use cad_occt::Shape;
use manifold_rust::manifold::Manifold;
use manifold_rust::types::{BooleanEngine, Error, MeshGL64};

use crate::geom::P3;

/// Malla cerrada de una pieza: vértices (mm) y triángulos hacia afuera.
pub type MeshData = Arc<(Vec<P3>, Vec<[u32; 3]>)>;

/// Con más triángulos que esto, una malla que entra a una booleana con una
/// pieza CAD va por manifold (y la pieza pasa a ser de malla); con menos, por
/// OCCT, que deja las caras de la pieza exactas (se pueden redondear).
pub const MESH_BOOLEAN_FACES: usize = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshOp {
    Union,
    Difference,
    Intersection,
}

/// La forma teselada fino y con los vértices de cada cara soldados (OCCT
/// repite los vértices en cada cara).
pub fn shape_mesh(shape: &Shape) -> Result<MeshData, String> {
    let (lo, hi) = shape.bounds().map_err(|e| e.to_string())?;
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
    let t = shape.tessellate((size * 2e-4).clamp(0.002, 0.05), 0.1).map_err(|e| e.to_string())?;
    Ok(Arc::new(weld(&t.positions, &t.triangles)))
}

/// Une los vértices en el mismo lugar (hasta 1e-9 del tamaño) y quita los
/// triángulos que quedan sin área de índices.
pub fn weld(v: &[P3], t: &[[u32; 3]]) -> (Vec<P3>, Vec<[u32; 3]>) {
    let (lo, hi) = bounds(v);
    let q = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max).max(1e-9) * 1e-9;
    let mut map: HashMap<[i64; 3], u32> = HashMap::with_capacity(v.len());
    let mut out = Vec::with_capacity(v.len());
    let remap: Vec<u32> = v
        .iter()
        .map(|p| {
            *map.entry(p.map(|c| (c / q).round() as i64)).or_insert_with(|| {
                out.push(*p);
                (out.len() - 1) as u32
            })
        })
        .collect();
    let tris = t.iter().map(|t| t.map(|i| remap[i as usize])).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    (out, tris)
}

fn bounds(v: &[P3]) -> (P3, P3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in v {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn mesh_gl(m: &MeshData) -> MeshGL64 {
    MeshGL64 {
        num_prop: 3,
        vert_properties: m.0.iter().flatten().copied().collect(),
        tri_verts: m.1.iter().flatten().map(|&i| i as u64).collect(),
        ..Default::default()
    }
}

/// La malla para manifold: estricta si es manifold limpia; si no (un escaneo
/// con aristas mal cosidas), por el camino robusto, que acepta cualquier
/// malla cerrada.
fn to_manifold(m: &MeshData) -> Result<Manifold, String> {
    let gl = mesh_gl(m);
    let strict = Manifold::from_mesh_gl64(&gl);
    if strict.status() == Error::NoError {
        return Ok(strict);
    }
    let robust = Manifold::from_mesh_gl64_robust(&gl);
    match robust.status() {
        Error::NoError => Ok(robust),
        Error::NotClosed => Err("la malla no está cerrada (repárala en Preparar)".into()),
        e => Err(format!("la malla no sirve para la booleana ({e:?})")),
    }
}

fn from_manifold(m: &Manifold) -> MeshData {
    let gl = m.get_mesh_gl64(0);
    let np = (gl.num_prop as usize).max(3);
    let v: Vec<P3> = gl.vert_properties.chunks(np).map(|c| [c[0], c[1], c[2]]).collect();
    let t: Vec<[u32; 3]> = gl.tri_verts.chunks(3).map(|c| [c[0] as u32, c[1] as u32, c[2] as u32]).collect();
    Arc::new((v, t))
}

/// `a` op `b` (para unir, también más mallas en `others`). Devuelve las piezas
/// sueltas que salen, la más grande primero, sin las astillas (menos de una
/// millonésima del volumen de la mayor). Vacío si no queda nada.
pub fn boolean(a: &MeshData, b: &[MeshData], op: MeshOp) -> Result<Vec<MeshData>, String> {
    let mut acc = to_manifold(a)?;
    for m in b {
        let other = to_manifold(m)?;
        acc = match op {
            MeshOp::Union => acc.union_with_engine(&other, BooleanEngine::Auto),
            MeshOp::Difference => acc.difference_with_engine(&other, BooleanEngine::Auto),
            MeshOp::Intersection => acc.intersection_with_engine(&other, BooleanEngine::Auto),
        };
        if acc.status() != Error::NoError {
            return Err(format!("la booleana de mallas falló ({:?})", acc.status()));
        }
    }
    if acc.is_empty() {
        return Ok(Vec::new());
    }
    let mut pieces: Vec<(f64, MeshData)> = acc.decompose().iter().map(|p| (p.volume().abs(), from_manifold(p))).collect();
    pieces.sort_by(|x, y| y.0.total_cmp(&x.0));
    let largest = pieces.first().map_or(0.0, |p| p.0);
    Ok(pieces.into_iter().filter(|(v, _)| *v >= largest * 1e-6 && *v > 0.0).map(|(_, m)| m).collect())
}

/// La malla como sólido OCCT (una cara por triángulo), para lo que sigue
/// necesitando OCCT: elegir la pieza, medir, operaciones sobre caras.
pub fn to_shape(m: &MeshData) -> Result<Shape, String> {
    Shape::from_mesh(&m.0, &m.1, 1e-4).map_err(|e| e.to_string())
}

/// Volumen, área, centro, caja e inercia (densidad 1, respecto al centro de
/// masa; momentos y ejes principales) de una malla cerrada, como
/// `Shape::mass` pero sin OCCT: en un sólido de 163 000 caras OCCT tardaba 18 s,
/// esto milisegundos. Integrales de Eberly («Polyhedral Mass Properties»).
pub fn mass(m: &MeshData) -> cad_occt::MassInfo {
    let (v, t) = (&m.0, &m.1);
    let sub = |w0: f64, w1: f64, w2: f64| {
        let t0 = w0 + w1;
        let f1 = t0 + w2;
        let t1 = w0 * w0;
        let t2 = t1 + w1 * t0;
        let f2 = t2 + w2 * f1;
        let f3 = w0 * t1 + w1 * t2 + w2 * f2;
        (f1, f2, f3, f2 + w0 * (f1 + w0), f2 + w1 * (f1 + w1), f2 + w2 * (f1 + w2))
    };
    let mut i = [0.0f64; 10];
    let mut area = 0.0;
    for tri in t {
        let [p0, p1, p2] = tri.map(|k| v[k as usize]);
        let (a1, b1, c1) = (p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
        let (a2, b2, c2) = (p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]);
        let (d0, d1, d2) = (b1 * c2 - b2 * c1, a2 * c1 - a1 * c2, a1 * b2 - a2 * b1);
        area += (d0 * d0 + d1 * d1 + d2 * d2).sqrt() / 2.0;
        let (f1x, f2x, f3x, g0x, g1x, g2x) = sub(p0[0], p1[0], p2[0]);
        let (_, f2y, f3y, g0y, g1y, g2y) = sub(p0[1], p1[1], p2[1]);
        let (_, f2z, f3z, g0z, g1z, g2z) = sub(p0[2], p1[2], p2[2]);
        i[0] += d0 * f1x;
        i[1] += d0 * f2x;
        i[2] += d1 * f2y;
        i[3] += d2 * f2z;
        i[4] += d0 * f3x;
        i[5] += d1 * f3y;
        i[6] += d2 * f3z;
        i[7] += d0 * (p0[1] * g0x + p1[1] * g1x + p2[1] * g2x);
        i[8] += d1 * (p0[2] * g0y + p1[2] * g1y + p2[2] * g2y);
        i[9] += d2 * (p0[0] * g0z + p1[0] * g1z + p2[0] * g2z);
    }
    let mult = [1.0 / 6.0, 1.0 / 24.0, 1.0 / 24.0, 1.0 / 24.0, 1.0 / 60.0, 1.0 / 60.0, 1.0 / 60.0, 1.0 / 120.0, 1.0 / 120.0, 1.0 / 120.0];
    for k in 0..10 {
        i[k] *= mult[k];
    }
    let volume = i[0];
    let c = if volume.abs() > 0.0 { [i[1] / volume, i[2] / volume, i[3] / volume] } else { [0.0; 3] };
    let ixx = i[5] + i[6] - volume * (c[1] * c[1] + c[2] * c[2]);
    let iyy = i[4] + i[6] - volume * (c[2] * c[2] + c[0] * c[0]);
    let izz = i[4] + i[5] - volume * (c[0] * c[0] + c[1] * c[1]);
    let ixy = -(i[7] - volume * c[0] * c[1]);
    let iyz = -(i[8] - volume * c[1] * c[2]);
    let ixz = -(i[9] - volume * c[2] * c[0]);
    let tensor = nalgebra::Matrix3::new(ixx, ixy, ixz, ixy, iyy, iyz, ixz, iyz, izz);
    let eig = nalgebra::SymmetricEigen::new(tensor);
    let axis = |k: usize| [eig.eigenvectors[(0, k)], eig.eigenvectors[(1, k)], eig.eigenvectors[(2, k)]];
    let (bbox_min, bbox_max) = bounds(v);
    cad_occt::MassInfo {
        volume,
        area,
        center: c,
        bbox_min,
        bbox_max,
        inertia: [eig.eigenvalues[0], eig.eigenvalues[1], eig.eigenvalues[2]],
        axes: [axis(0), axis(1), axis(2)],
    }
}

/// Masa de varias piezas juntas (cada una de OCCT o de [`mass`]): volúmenes y
/// áreas sumados, caja que las abarca, centro ponderado e inercia con el
/// teorema de Steiner respecto al centro común.
pub fn combine_mass(items: &[cad_occt::MassInfo]) -> Option<cad_occt::MassInfo> {
    let first = items.first()?;
    if items.len() == 1 {
        return Some(*first);
    }
    let volume: f64 = items.iter().map(|m| m.volume).sum();
    let area: f64 = items.iter().map(|m| m.area).sum();
    let center = if volume.abs() > 0.0 { [0, 1, 2].map(|k| items.iter().map(|m| m.volume * m.center[k]).sum::<f64>() / volume) } else { first.center };
    let mut bbox_min = first.bbox_min;
    let mut bbox_max = first.bbox_max;
    let mut tensor = nalgebra::Matrix3::<f64>::zeros();
    for m in items {
        for k in 0..3 {
            bbox_min[k] = bbox_min[k].min(m.bbox_min[k]);
            bbox_max[k] = bbox_max[k].max(m.bbox_max[k]);
        }
        let r = nalgebra::Matrix3::from_columns(&m.axes.map(|a| nalgebra::Vector3::new(a[0], a[1], a[2])));
        let own = r * nalgebra::Matrix3::from_diagonal(&nalgebra::Vector3::new(m.inertia[0], m.inertia[1], m.inertia[2])) * r.transpose();
        let d = nalgebra::Vector3::new(m.center[0] - center[0], m.center[1] - center[1], m.center[2] - center[2]);
        tensor += own + (nalgebra::Matrix3::identity() * d.dot(&d) - d * d.transpose()) * m.volume;
    }
    let eig = nalgebra::SymmetricEigen::new(tensor);
    let axis = |k: usize| [eig.eigenvectors[(0, k)], eig.eigenvectors[(1, k)], eig.eigenvectors[(2, k)]];
    Some(cad_occt::MassInfo {
        volume,
        area,
        center,
        bbox_min,
        bbox_max,
        inertia: [eig.eigenvalues[0], eig.eigenvalues[1], eig.eigenvalues[2]],
        axes: [axis(0), axis(1), axis(2)],
    })
}

/// Piezas sueltas de la malla (por aristas compartidas).
pub fn components(m: &MeshData) -> usize {
    let n = m.0.len();
    let mut parent: Vec<u32> = (0..n as u32).collect();
    fn root(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for t in &m.1 {
        let a = root(&mut parent, t[0]);
        for &k in &t[1..] {
            let b = root(&mut parent, k);
            parent[b as usize] = a;
        }
    }
    let used: std::collections::HashSet<u32> = m.1.iter().flatten().map(|&k| root(&mut parent, k)).collect();
    used.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_occt::Frame;

    fn cube_mesh(o: P3, s: f64) -> MeshData {
        let shape = Shape::make_box(Frame { origin: o, ..Frame::WORLD }, s, s, s).unwrap();
        shape_mesh(&shape).unwrap()
    }

    fn volume(m: &MeshData) -> f64 {
        m.1.iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| m.0[i as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
            })
            .sum()
    }

    #[test]
    fn difference_union_intersection_of_cubes() {
        let a = cube_mesh([0.0; 3], 10.0);
        let b = cube_mesh([5.0, 5.0, 5.0], 10.0);
        let d = boolean(&a, &[b.clone()], MeshOp::Difference).unwrap();
        assert_eq!(d.len(), 1);
        assert!((volume(&d[0]) - 875.0).abs() < 1e-6, "{}", volume(&d[0]));
        let u = boolean(&a, &[b.clone()], MeshOp::Union).unwrap();
        assert!((volume(&u[0]) - 1875.0).abs() < 1e-6);
        let i = boolean(&a, &[b], MeshOp::Intersection).unwrap();
        assert!((volume(&i[0]) - 125.0).abs() < 1e-6);
        // De vuelta a OCCT: un sólido con el mismo volumen
        let s = to_shape(&d[0]).unwrap();
        assert!((s.mass().unwrap().volume - 875.0).abs() < 1e-6);
    }

    #[test]
    fn cut_that_splits_gives_pieces() {
        // Una barra cortada por el medio por una placa: dos piezas
        let bar = Arc::new(weld(
            &shape_mesh(&Shape::make_box(Frame::WORLD, 30.0, 4.0, 4.0).unwrap()).unwrap().0,
            &shape_mesh(&Shape::make_box(Frame::WORLD, 30.0, 4.0, 4.0).unwrap()).unwrap().1,
        ));
        let plate = shape_mesh(&Shape::make_box(Frame { origin: [14.0, -1.0, -1.0], ..Frame::WORLD }, 2.0, 6.0, 6.0).unwrap()).unwrap();
        let pieces = boolean(&bar, &[plate], MeshOp::Difference).unwrap();
        assert_eq!(pieces.len(), 2);
        assert!((volume(&pieces[0]) - 14.0 * 16.0).abs() < 1e-6);
    }

    /// La masa por triángulos da lo mismo que OCCT (caja desplazada y girada en la malla)
    #[test]
    fn mass_matches_occt() {
        let shape = Shape::make_box(Frame { origin: [3.0, -2.0, 5.0], z: [0.0, 0.6, 0.8], x: [1.0, 0.0, 0.0] }, 10.0, 4.0, 2.0).unwrap();
        let m = shape_mesh(&shape).unwrap();
        let ours = mass(&m);
        let occt = shape.mass().unwrap();
        assert!((ours.volume - occt.volume).abs() < 1e-6, "{} {}", ours.volume, occt.volume);
        assert!((ours.area - occt.area).abs() < 1e-6);
        for k in 0..3 {
            assert!((ours.center[k] - occt.center[k]).abs() < 1e-6);
        }
        let mut a = ours.inertia;
        let mut b = occt.inertia;
        a.sort_by(f64::total_cmp);
        b.sort_by(f64::total_cmp);
        for k in 0..3 {
            assert!((a[k] - b[k]).abs() < 1e-6 * b[2], "{a:?} {b:?}");
        }
        assert_eq!(components(&m), 1);
        // Dos cajas juntas: igual que OCCT con el compuesto
        let other = Shape::make_box(Frame { origin: [20.0, 1.0, 0.0], ..Frame::WORLD }, 3.0, 5.0, 7.0).unwrap();
        let both = combine_mass(&[ours, other.mass().unwrap()]).unwrap();
        let occt_both = Shape::compound(&[shape, other]).unwrap().mass().unwrap();
        assert!((both.volume - occt_both.volume).abs() < 1e-6);
        let mut a = both.inertia;
        let mut b = occt_both.inertia;
        a.sort_by(f64::total_cmp);
        b.sort_by(f64::total_cmp);
        for k in 0..3 {
            assert!((a[k] - b[k]).abs() < 1e-6 * b[2], "{a:?} {b:?}");
        }
    }

    /// `to_shape` deja una cara por triángulo y en el mismo orden (la vista
    /// muestra los triángulos de la pieza de malla con el número de su cara)
    #[test]
    fn shape_faces_follow_triangles() {
        let m = shape_mesh(&Shape::sphere([1.0, 2.0, 3.0], 5.0).unwrap()).unwrap();
        let s = to_shape(&m).unwrap();
        assert_eq!(s.face_count(), m.1.len());
        for (k, t) in m.1.iter().enumerate().step_by(37) {
            let c = t.iter().fold([0.0; 3], |acc, &i| [0, 1, 2].map(|j| acc[j] + m.0[i as usize][j] / 3.0));
            let f = s.face_info(k).unwrap().center;
            assert!((0..3).all(|j| (c[j] - f[j]).abs() < 1e-9), "cara {k}: {f:?} vs {c:?}");
        }
    }

    #[test]
    fn disjoint_intersection_is_empty() {
        let a = cube_mesh([0.0; 3], 1.0);
        let b = cube_mesh([5.0, 0.0, 0.0], 1.0);
        assert!(boolean(&a, &[b], MeshOp::Intersection).unwrap().is_empty());
    }
}
