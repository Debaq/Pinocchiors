//! Zona guardada por su lugar en el espacio, para volver a encontrarla en otra malla del mismo modelo
//!
//! Cuando el diseño cambia, la pieza se vuelve a teselar y los números de cara ya no sirven. Por eso, al aplicar la
//! piel se guarda qué celdas de una rejilla toca la superficie de la zona (con la normal media de cada celda), cuáles
//! toca lo protegido y la intensidad pintada. En la malla nueva, una cara es de la zona si su centro cae en una celda
//! de la zona y mira hacia el mismo lado.

use std::collections::HashMap;

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;
use serde::{Deserialize, Serialize};

/// Zona, protegidas e intensidad, guardadas en una rejilla (unidades de la malla)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ZoneRegion {
    /// Lado de la celda
    pub cell: Real,
    /// Celdas de la zona: x, y, z y la normal media por 100 (seis enteros por celda)
    pub zone: Vec<i32>,
    /// Celdas de lo protegido, igual que `zone`
    #[serde(default)]
    pub protected: Vec<i32>,
    /// Celdas con intensidad menor que el 100 %: x, y, z y la intensidad por 1000 (cuatro enteros por celda)
    #[serde(default)]
    pub strength: Vec<i32>,
}

type Key = (i32, i32, i32);

fn key(p: Vector3, cell: Real) -> Key {
    ((p.x() / cell).floor() as i32, (p.y() / cell).floor() as i32, (p.z() / cell).floor() as i32)
}

/// Puntos sobre el triángulo separados menos de media celda, con sus coordenadas baricéntricas
fn samples(t: [Vector3; 3], cell: Real, mut visit: impl FnMut(Vector3, [Real; 3])) {
    let longest = (t[1] - t[0]).length().max((t[2] - t[1]).length()).max((t[0] - t[2]).length());
    let n = ((longest / (cell * 0.5)).ceil() as usize).clamp(1, 256);
    for i in 0..=n {
        for j in 0..=n - i {
            let (a, b) = (i as Real / n as Real, j as Real / n as Real);
            let w = [1.0 - a - b, a, b];
            visit(t[0] * w[0] + t[1] * w[1] + t[2] * w[2], w);
        }
    }
}

fn tri(mesh: &Mesh, f: usize) -> ([usize; 3], [Vector3; 3]) {
    let v = mesh.get_face_vertices(f);
    let v = [v[0], v[1], v[2]];
    (v, v.map(|i| mesh.vertices[i].position))
}

fn cells_of(mesh: &Mesh, faces: &[usize], cell: Real) -> Vec<i32> {
    let mut acc: HashMap<Key, Vector3> = HashMap::new();
    for &f in faces.iter().filter(|&&f| f < mesh.num_faces()) {
        let (_, t) = tri(mesh, f);
        let n = mesh.get_face_normal(f);
        samples(t, cell, |p, _| *acc.entry(key(p, cell)).or_insert_with(Vector3::zero) += n);
    }
    let mut keys: Vec<_> = acc.into_iter().collect();
    keys.sort_by_key(|(k, _)| *k);
    keys.into_iter()
        .flat_map(|((x, y, z), n)| {
            let n = n.try_normalize().unwrap_or_else(Vector3::zero);
            [x, y, z, (n.x() * 100.0).round() as i32, (n.y() * 100.0).round() as i32, (n.z() * 100.0).round() as i32]
        })
        .collect()
}

impl ZoneRegion {
    /// Guarda la zona `zone`, las caras `protected` y la intensidad por vértice `strength` (vacía = todo al 100 %)
    pub fn capture(mesh: &Mesh, zone: &[usize], protected: &[usize], strength: &[f32], cell: Real) -> Self {
        let cell = if cell.is_finite() && cell > 0.0 { cell } else { 1.0 };
        let mut out = Self { cell, zone: cells_of(mesh, zone, cell), protected: cells_of(mesh, protected, cell), strength: Vec::new() };
        if strength.len() == mesh.vertices.len() && strength.iter().any(|&s| s < 0.999) {
            let mut acc: HashMap<Key, (Real, u32)> = HashMap::new();
            for &f in zone.iter().filter(|&&f| f < mesh.num_faces()) {
                let (v, t) = tri(mesh, f);
                let s = v.map(|i| Real::from(strength[i]).clamp(0.0, 1.0));
                samples(t, cell, |p, w| {
                    let e = acc.entry(key(p, cell)).or_insert((0.0, 0));
                    e.0 += s[0] * w[0] + s[1] * w[1] + s[2] * w[2];
                    e.1 += 1;
                });
            }
            let mut keys: Vec<_> = acc.into_iter().map(|(k, (sum, n))| (k, sum / Real::from(n))).filter(|(_, s)| *s < 0.999).collect();
            keys.sort_by_key(|(k, _)| *k);
            out.strength = keys.into_iter().flat_map(|((x, y, z), s)| [x, y, z, (s * 1000.0).round() as i32]).collect();
        }
        out
    }

    /// Las caras de `mesh` que caen en las celdas de `cells` mirando hacia el mismo lado
    fn faces_in(&self, mesh: &Mesh, cells: &[i32]) -> Vec<usize> {
        let map: HashMap<Key, Vector3> = cells
            .as_chunks::<6>()
            .0
            .iter()
            .map(|c| ((c[0], c[1], c[2]), Vector3::new(Real::from(c[3]), Real::from(c[4]), Real::from(c[5])) * 0.01))
            .collect();
        if map.is_empty() {
            return Vec::new();
        }
        (0..mesh.num_faces())
            .filter(|&f| {
                let (_, t) = tri(mesh, f);
                let c = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
                let normal = mesh.get_face_normal(f);
                // Un centro justo en el borde de una celda puede caer a cualquier lado por el redondeo: se mira un
                // pelo hacia cada lado
                let e = self.cell * 1e-3;
                let hit = |p: Vector3| map.get(&key(p, self.cell)).is_some_and(|n| normal.dot(n) > 0.5);
                hit(c) || (0..8).any(|k| hit(c + Vector3::new(if k & 1 == 0 { -e } else { e }, if k & 2 == 0 { -e } else { e }, if k & 4 == 0 { -e } else { e })))
            })
            .collect()
    }

    /// Zona y protegidas en `mesh`, y la intensidad de cada vértice (vacía si todo quedó al 100 %)
    pub fn locate(&self, mesh: &Mesh) -> (Vec<usize>, Vec<usize>, Vec<f32>) {
        let zone = self.faces_in(mesh, &self.zone);
        let protected = self.faces_in(mesh, &self.protected);
        let strength = if self.strength.is_empty() {
            Vec::new()
        } else {
            let map: HashMap<Key, f32> = self.strength.as_chunks::<4>().0.iter().map(|c| ((c[0], c[1], c[2]), c[3] as f32 / 1000.0)).collect();
            mesh.vertices.iter().map(|v| map.get(&key(v.position, self.cell)).copied().unwrap_or(1.0)).collect()
        };
        (zone, protected, strength)
    }

    /// No guarda ninguna celda de la zona
    pub fn is_empty(&self) -> bool {
        self.zone.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printability::tests::make_box;

    /// La misma caja partida más fino (otra teselación del mismo modelo)
    fn finer(m: &Mesh) -> Mesh {
        let (pos, tris) = crate::geom::triangles(m);
        let mut p = pos.clone();
        let mut out = Vec::new();
        for t in tris {
            let c = p.len();
            p.push((pos[t[0]] + pos[t[1]] + pos[t[2]]) * (1.0 / 3.0));
            out.push([t[0], t[1], c]);
            out.push([t[1], t[2], c]);
            out.push([t[2], t[0], c]);
        }
        Mesh::from_triangles(&p, &out)
    }

    fn top(m: &Mesh, z: Real) -> Vec<usize> {
        (0..m.num_faces()).filter(|&f| m.get_face_vertices(f).iter().all(|&i| (m.vertices[i].position.z() - z).abs() < 1e-9)).collect()
    }

    #[test]
    fn la_zona_se_encuentra_en_otra_teselacion() {
        let b = make_box(40.0, 20.0, 10.0);
        let zone = top(&b, 10.0);
        let bottom = (0..b.num_faces()).filter(|f| !zone.contains(f)).filter(|&f| b.get_face_normal(f).z() < -0.9).collect::<Vec<_>>();
        let region = ZoneRegion::capture(&b, &zone, &bottom, &[], 1.0);
        assert!(!region.is_empty() && region.strength.is_empty());
        let fine = finer(&b);
        let (z, p, s) = region.locate(&fine);
        assert_eq!(z, top(&fine, 10.0), "la tapa de la malla nueva");
        assert_eq!(p, top(&fine, 0.0), "el fondo protegido");
        assert!(s.is_empty());
        // Las caras laterales que tocan la tapa no entran (miran hacia otro lado)
        assert!(z.iter().all(|&f| fine.get_face_normal(f).z() > 0.9));
    }

    #[test]
    fn la_intensidad_viaja_por_posicion() {
        let b = make_box(40.0, 20.0, 10.0);
        let zone = top(&b, 10.0);
        let strength: Vec<f32> = b.vertices.iter().map(|v| (v.position.x() / 40.0) as f32).collect();
        let region = ZoneRegion::capture(&b, &zone, &[], &strength, 0.5);
        assert!(!region.strength.is_empty());
        let fine = finer(&b);
        let (_, _, s) = region.locate(&fine);
        assert_eq!(s.len(), fine.vertices.len());
        // En la tapa, la intensidad sigue siendo ≈ x / 40 (dentro de una celda)
        for (v, &k) in fine.vertices.iter().zip(&s) {
            if (v.position.z() - 10.0).abs() < 1e-9 && v.position.x() > 1.0 && v.position.x() < 39.0 {
                assert!((k as Real - v.position.x() / 40.0).abs() < 0.03, "x = {} → {k}", v.position.x());
            }
        }
        // Se guarda y se lee igual
        let json = serde_json::to_string(&region).unwrap();
        assert_eq!(serde_json::from_str::<ZoneRegion>(&json).unwrap(), region);
    }
}
