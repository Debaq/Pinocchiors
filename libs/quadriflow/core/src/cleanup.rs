//! Limpieza topológica de la malla poligonal antes de dividirla en quads.
//!
//! Cada polígono de n lados termina como un vértice de valencia n (su centro)
//! y cada vértice conserva su valencia, así que la irregularidad final se mide
//! con E = Σ (valencia − 4)² + Σ (lados − 4)². Se aplican, mientras bajen E y
//! dejen polígonos válidos, tres operaciones locales:
//!
//! - **Disolver una arista**: dos caras vecinas se funden (dos triángulos → un
//!   quad; triángulo + pentágono junto a un vértice de valencia 5…).
//! - **Disolver un vértice de valencia 2** (*doublet*): sus dos caras se funden.
//! - **Colapsar la diagonal de un quad**: sus extremos se funden (pares 3-3,
//!   o 3-5 con vecinos de valencia 5).
//!
//! Antes, las aristas mucho más cortas que el resto (dos vértices fijos en la
//! misma esquina de una arista viva) se colapsan aunque suba E.
//!
//! No se tocan vértices de borde ni se funden dos vértices fijos (aristas
//! vivas) salvo en ese colapso, y ninguna cara puede quedar plegada.

use crate::extract::Polygons;
use crate::V3;

/// Pasadas máximas sobre toda la malla.
const MAX_PASSES: usize = 10;

/// Fracción del largo medio de arista por debajo de la cual se colapsa: más
/// generosa entre dos vértices fijos (restos de una misma esquina viva).
const SHORT_EDGE: f64 = 0.01;
const SHORT_FIXED_EDGE: f64 = 0.1;

/// Ninguna operación deja una esquina más cerrada que esto (ni más abierta que
/// su suplemento), salvo que la zona ya estuviera peor.
const MIN_CORNER_ANGLE: f64 = 30.0;

pub(crate) fn simplify(poly: &mut Polygons) {
    split_repeated(poly);
    let mut mesh = Work::new(poly);
    let average = mesh.average_edge_length();
    for f in 0..mesh.faces.len() {
        while mesh.try_collapse_short_edge(f, SHORT_EDGE * average, SHORT_FIXED_EDGE * average) {}
    }
    for _ in 0..MAX_PASSES {
        let mut changed = false;
        for v in 0..mesh.faces_of.len() {
            changed |= mesh.try_dissolve_vertex(v as u32);
        }
        for f in 0..mesh.faces.len() {
            changed |= mesh.try_collapse_diagonal(f);
        }
        for f in 0..mesh.faces.len() {
            changed |= mesh.try_dissolve_edges(f);
        }
        if !changed {
            break;
        }
    }
    poly.faces = mesh.faces.into_iter().flatten().collect();
    poly.vertices = mesh.pos;
}

/// Parte los polígonos que pasan dos veces por un vértice en ciclos simples.
fn split_repeated(poly: &mut Polygons) {
    let faces = std::mem::take(&mut poly.faces);
    for f in faces {
        let mut sorted = f.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() == f.len() {
            poly.faces.push(f);
        } else {
            poly.faces.extend(crate::extract::simple_cycles(&f).into_iter().filter(|c| c.len() >= 3));
        }
    }
}

struct Work {
    pos: Vec<V3>,
    fixed: Vec<bool>,
    boundary: Vec<bool>,
    faces: Vec<Option<Vec<u32>>>,
    faces_of: Vec<Vec<usize>>,
}

fn dev(x: usize) -> i64 {
    (x as i64 - 4).pow(2)
}

impl Work {
    fn new(poly: &Polygons) -> Self {
        let n = poly.vertices.len();
        let mut faces_of = vec![Vec::new(); n];
        let mut edges: std::collections::HashMap<(u32, u32), u32> = Default::default();
        for (i, f) in poly.faces.iter().enumerate() {
            for k in 0..f.len() {
                faces_of[f[k] as usize].push(i);
                let (a, b) = (f[k], f[(k + 1) % f.len()]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let mut boundary = vec![false; n];
        for (&(a, b), &c) in &edges {
            if c != 2 {
                boundary[a as usize] = true;
                boundary[b as usize] = true;
            }
        }
        Self {
            pos: poly.vertices.clone(),
            fixed: poly.fixed.clone(),
            boundary,
            faces: poly.faces.iter().cloned().map(Some).collect(),
            faces_of,
        }
    }

    fn valence(&self, v: u32) -> usize {
        self.faces_of[v as usize].len()
    }

    fn face(&self, f: usize) -> &[u32] {
        self.faces[f].as_deref().unwrap_or(&[])
    }

    /// La otra cara que usa la arista `a → b` en sentido `b → a`.
    fn twin(&self, a: u32, b: u32) -> Option<usize> {
        self.faces_of[b as usize].iter().copied().find(|&g| {
            let f = self.face(g);
            (0..f.len()).any(|k| f[k] == b && f[(k + 1) % f.len()] == a)
        })
    }

    /// Normal de Newell (no normalizada).
    fn normal(&self, f: &[u32]) -> V3 {
        let mut n = V3::zeros();
        for k in 0..f.len() {
            let (p, q) = (self.pos[f[k] as usize], self.pos[f[(k + 1) % f.len()] as usize]);
            n += p.cross(&q);
        }
        n
    }

    /// El polígono es estrellado desde su centroide y no está plegado respecto
    /// de `reference`: condición para dividirlo en quads por su centro.
    fn valid(&self, f: &[u32], reference: &V3) -> bool {
        let n = self.normal(f);
        if n.dot(reference) <= 0.0 {
            return false;
        }
        let c = f.iter().map(|&v| self.pos[v as usize]).sum::<V3>() / f.len() as f64;
        (0..f.len()).all(|k| {
            let (a, b) = (self.pos[f[k] as usize] - c, self.pos[f[(k + 1) % f.len()] as usize] - c);
            a.cross(&b).dot(&n) > 0.0
        })
    }

    /// Peor esquina del polígono: min(ángulo, 180° − ángulo), en grados.
    fn quality(&self, f: &[u32]) -> f64 {
        (0..f.len())
            .map(|k| {
                let p = self.pos[f[k] as usize];
                let a = self.pos[f[(k + 1) % f.len()] as usize] - p;
                let b = self.pos[f[(k + f.len() - 1) % f.len()] as usize] - p;
                let angle = a.angle(&b).to_degrees();
                angle.min(180.0 - angle)
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// Las caras nuevas no tienen esquinas peores que el umbral o que las viejas.
    fn good_enough(&self, old: &[&[u32]], new: &[&[u32]], old_quality: Option<f64>) -> bool {
        let before = old_quality.unwrap_or_else(|| old.iter().map(|f| self.quality(f)).fold(f64::INFINITY, f64::min));
        let after = new.iter().map(|f| self.quality(f)).fold(f64::INFINITY, f64::min);
        after >= before.min(MIN_CORNER_ANGLE)
    }

    fn replace_face(&mut self, f: usize, new: Option<Vec<u32>>) {
        for &v in self.face(f).to_vec().iter() {
            self.faces_of[v as usize].retain(|&g| g != f);
        }
        if let Some(face) = &new {
            for &v in face {
                self.faces_of[v as usize].push(f);
            }
        }
        self.faces[f] = new;
    }

    /// Fusiona las caras `f` y `g` (que comparten solo la arista `u → v` de `f`).
    fn merged(&self, f: usize, g: usize, u: u32, v: u32) -> Option<Vec<u32>> {
        let (ff, gg) = (self.face(f), self.face(g));
        // f desde v hasta u; g desde u hasta v
        let kf = ff.iter().position(|&x| x == v)?;
        let kg = gg.iter().position(|&x| x == u)?;
        let mut out: Vec<u32> = (0..ff.len()).map(|i| ff[(kf + i) % ff.len()]).collect();
        out.extend((1..gg.len() - 1).map(|i| gg[(kg + i) % gg.len()]));
        let mut sorted = out.clone();
        sorted.sort_unstable();
        sorted.dedup();
        (sorted.len() == out.len() && out.len() >= 3).then_some(out)
    }

    fn try_dissolve_edges(&mut self, f: usize) -> bool {
        let Some(face) = self.faces[f].clone() else { return false };
        for k in 0..face.len() {
            let (u, v) = (face[k], face[(k + 1) % face.len()]);
            if self.boundary[u as usize] || self.boundary[v as usize] {
                continue;
            }
            if self.fixed[u as usize] && self.fixed[v as usize] {
                continue; // arista sobre una arista viva
            }
            let Some(g) = self.twin(u, v) else { continue };
            if g == f {
                continue;
            }
            let (nf, ng) = (face.len(), self.face(g).len());
            let (vu, vv) = (self.valence(u), self.valence(v));
            let delta = dev(nf + ng - 2) - dev(nf) - dev(ng) + dev(vu - 1) - dev(vu) + dev(vv - 1) - dev(vv);
            if delta >= 0 || vu <= 2 || vv <= 2 {
                continue;
            }
            let Some(new) = self.merged(f, g, u, v) else { continue };
            let reference = self.normal(&face) + self.normal(self.face(g));
            if !self.valid(&new, &reference) || !self.good_enough(&[&face, self.face(g)], &[&new], None) {
                continue;
            }
            self.replace_face(g, None);
            self.replace_face(f, Some(new));
            return true;
        }
        false
    }

    fn try_dissolve_vertex(&mut self, v: u32) -> bool {
        if self.valence(v) != 2 || self.boundary[v as usize] || self.fixed[v as usize] {
            return false;
        }
        let (f, g) = (self.faces_of[v as usize][0], self.faces_of[v as usize][1]);
        let (ff, gg) = (self.face(f).to_vec(), self.face(g).to_vec());
        let kf = ff.iter().position(|&x| x == v).expect("v en f");
        let (a, b) = (ff[(kf + ff.len() - 1) % ff.len()], ff[(kf + 1) % ff.len()]);
        // f = … a v b …; g debe ser … b v a …
        let kg = gg.iter().position(|&x| x == v).expect("v en g");
        if gg[(kg + gg.len() - 1) % gg.len()] != b || gg[(kg + 1) % gg.len()] != a {
            return false;
        }
        let (va, vb) = (self.valence(a), self.valence(b));
        let n = ff.len() + gg.len() - 4;
        if n < 3 {
            return false;
        }
        let delta = dev(n) - dev(ff.len()) - dev(gg.len()) - dev(2) + dev(va - 1) - dev(va) + dev(vb - 1) - dev(vb);
        if delta >= 0 {
            return false;
        }
        // f desde b hasta a, luego g desde a hasta b (sin v ni repetir a y b)
        let kb = ff.iter().position(|&x| x == b).expect("b en f");
        let mut new: Vec<u32> = (0..ff.len() - 1).map(|i| ff[(kb + i) % ff.len()]).collect();
        let ka = gg.iter().position(|&x| x == a).expect("a en g");
        new.extend((1..gg.len() - 2).map(|i| gg[(ka + i) % gg.len()]));
        let mut sorted = new.clone();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.len() != new.len() {
            return false;
        }
        let reference = self.normal(&ff) + self.normal(&gg);
        if !self.valid(&new, &reference) || !self.good_enough(&[&ff, &gg], &[&new], None) {
            return false;
        }
        self.replace_face(g, None);
        self.replace_face(f, Some(new));
        true
    }

    fn average_edge_length(&self) -> f64 {
        let (mut total, mut count) = (0.0, 0usize);
        for f in self.faces.iter().flatten() {
            for k in 0..f.len() {
                total += (self.pos[f[k] as usize] - self.pos[f[(k + 1) % f.len()] as usize]).norm();
                count += 1;
            }
        }
        if count == 0 { 0.0 } else { total / count as f64 }
    }

    /// Colapsa la primera arista de `f` más corta que `min_len` (`min_fixed`
    /// si sus dos extremos son fijos). Las caras que la contienen pierden un
    /// vértice (un triángulo desaparece).
    fn try_collapse_short_edge(&mut self, f: usize, min_len: f64, min_fixed: f64) -> bool {
        let Some(face) = self.faces[f].clone() else { return false };
        for k in 0..face.len() {
            let (u, v) = (face[k], face[(k + 1) % face.len()]);
            let limit = if self.fixed[u as usize] && self.fixed[v as usize] { min_fixed } else { min_len };
            if (self.pos[u as usize] - self.pos[v as usize]).norm() >= limit {
                continue;
            }
            if self.boundary[u as usize] != self.boundary[v as usize] {
                continue;
            }
            // Caras con ambos vértices: deben tenerlos consecutivos
            let shared: Vec<usize> =
                self.faces_of[u as usize].iter().copied().filter(|&g| self.face(g).contains(&v)).collect();
            let consecutive = |g: usize| {
                let h = self.face(g);
                (0..h.len()).any(|i| {
                    let (a, b) = (h[i], h[(i + 1) % h.len()]);
                    (a, b) == (u, v) || (a, b) == (v, u)
                })
            };
            if !shared.iter().all(|&g| consecutive(g)) {
                continue;
            }
            // Condición de enlace: los vecinos comunes son los terceros vértices
            // de los triángulos sobre la arista
            let mut apex: Vec<u32> = shared
                .iter()
                .filter(|&&g| self.face(g).len() == 3)
                .map(|&g| *self.face(g).iter().find(|&&x| x != u && x != v).expect("tercer vértice"))
                .collect();
            apex.sort_unstable();
            let (nu, nv) = (self.neighbors(u), self.neighbors(v));
            let common: Vec<u32> = nu.iter().copied().filter(|x| nv.binary_search(x).is_ok()).collect();
            if common != apex {
                continue;
            }

            let target = match (self.fixed[u as usize], self.fixed[v as usize]) {
                (_, true) if !self.fixed[u as usize] => self.pos[v as usize],
                (true, _) => self.pos[u as usize],
                _ => (self.pos[u as usize] + self.pos[v as usize]) * 0.5,
            };
            let affected: Vec<usize> = self.faces_of[u as usize]
                .iter()
                .chain(&self.faces_of[v as usize])
                .copied()
                .filter(|g| !shared.contains(g))
                .collect();
            let before: Vec<V3> = affected.iter().map(|&g| self.normal(self.face(g))).collect();
            let old = (self.pos[u as usize], self.pos[v as usize]);
            self.pos[u as usize] = target;
            self.pos[v as usize] = target;
            let renamed: Vec<Vec<u32>> = affected
                .iter()
                .map(|&g| self.face(g).iter().map(|&x| if x == v { u } else { x }).collect())
                .collect();
            if !renamed.iter().zip(&before).all(|(g, n)| self.valid(g, n)) {
                (self.pos[u as usize], self.pos[v as usize]) = old;
                continue;
            }
            self.fixed[u as usize] |= self.fixed[v as usize];
            for g in shared {
                let h: Vec<u32> = self.face(g).iter().copied().filter(|&x| x != v).collect();
                self.replace_face(g, (h.len() >= 3).then_some(h));
            }
            for (g, new) in affected.into_iter().zip(renamed) {
                self.replace_face(g, Some(new));
            }
            return true;
        }
        false
    }

    fn neighbors(&self, v: u32) -> Vec<u32> {
        let mut out: Vec<u32> = self.faces_of[v as usize]
            .iter()
            .flat_map(|&f| {
                let face = self.face(f);
                let k = face.iter().position(|&x| x == v).expect("v en la cara");
                [face[(k + 1) % face.len()], face[(k + face.len() - 1) % face.len()]]
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn try_collapse_diagonal(&mut self, f: usize) -> bool {
        let Some(face) = self.faces[f].clone() else { return false };
        if face.len() != 4 {
            return false;
        }
        for k in 0..2 {
            let (p, q) = (face[k], face[k + 2]);
            let (s, t) = (face[k + 1], face[(k + 3) % 4]);
            if [p, q, s, t].iter().any(|&x| self.boundary[x as usize]) {
                continue;
            }
            if self.fixed[p as usize] && self.fixed[q as usize] {
                continue;
            }
            let (vp, vq, vs, vt) = (self.valence(p), self.valence(q), self.valence(s), self.valence(t));
            if vs <= 2 || vt <= 2 {
                continue;
            }
            let delta = dev(vp + vq - 2) - dev(vp) - dev(vq) + dev(vs - 1) - dev(vs) + dev(vt - 1) - dev(vt)
                - dev(4);
            if delta >= 0 {
                continue;
            }
            // Condición de enlace: p y q solo comparten a s y t como vecinos
            let (np, nq) = (self.neighbors(p), self.neighbors(q));
            let common: Vec<u32> = np.iter().copied().filter(|x| nq.binary_search(x).is_ok()).collect();
            let mut st = vec![s, t];
            st.sort_unstable();
            if common != st {
                continue;
            }
            if self.faces_of[q as usize].iter().any(|&g| g != f && self.face(g).contains(&p)) {
                continue;
            }

            let target = if self.fixed[q as usize] {
                self.pos[q as usize]
            } else if self.fixed[p as usize] {
                self.pos[p as usize]
            } else {
                (self.pos[p as usize] + self.pos[q as usize]) * 0.5
            };
            let affected: Vec<usize> = self.faces_of[p as usize]
                .iter()
                .chain(&self.faces_of[q as usize])
                .copied()
                .filter(|&g| g != f)
                .collect();
            let before: Vec<V3> = affected.iter().map(|&g| self.normal(self.face(g))).collect();
            let old_quality = affected
                .iter()
                .map(|&g| self.quality(self.face(g)))
                .fold(self.quality(&face), f64::min);
            let old = (self.pos[p as usize], self.pos[q as usize]);
            self.pos[p as usize] = target;
            self.pos[q as usize] = target;
            let renamed: Vec<Vec<u32>> = affected
                .iter()
                .map(|&g| self.face(g).iter().map(|&x| if x == q { p } else { x }).collect())
                .collect();
            let new_faces: Vec<&[u32]> = renamed.iter().map(Vec::as_slice).collect();
            let ok = renamed.iter().zip(&before).all(|(g, n)| self.valid(g, n))
                && self.good_enough(&[], &new_faces, Some(old_quality));
            if !ok {
                (self.pos[p as usize], self.pos[q as usize]) = old;
                continue;
            }
            self.fixed[p as usize] |= self.fixed[q as usize];
            self.replace_face(f, None);
            for (g, new) in affected.into_iter().zip(renamed) {
                self.replace_face(g, Some(new));
            }
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grilla plana (n+1)×(n+1) de vértices en quads, con el contorno fijo.
    fn grid(n: u32) -> Polygons {
        let w = n + 1;
        let vertices = (0..w * w).map(|i| V3::new((i % w) as f64, (i / w) as f64, 0.0)).collect();
        let faces = (0..n * n)
            .map(|c| {
                let (x, y) = (c % n, c / n);
                let v = y * w + x;
                vec![v, v + 1, v + 1 + w, v + w]
            })
            .collect();
        Polygons { vertices, fixed: vec![false; (w * w) as usize], faces }
    }

    fn energy(poly: &Polygons) -> i64 {
        let work = Work::new(poly);
        let faces: i64 = poly.faces.iter().map(|f| dev(f.len())).sum();
        let verts: i64 = (0..poly.vertices.len() as u32)
            .filter(|&v| !work.boundary[v as usize] && work.valence(v) > 0)
            .map(|v| dev(work.valence(v)))
            .sum();
        faces + verts
    }

    #[test]
    fn two_triangles_become_a_quad() {
        // Grilla 4×4 con la celda central partida en dos triángulos
        let mut poly = grid(4);
        let cell = poly.faces.remove(5);
        poly.faces.push(vec![cell[0], cell[1], cell[2]]);
        poly.faces.push(vec![cell[0], cell[2], cell[3]]);
        assert!(energy(&poly) > 0);
        simplify(&mut poly);
        assert_eq!(energy(&poly), 0);
        assert!(poly.faces.iter().all(|f| f.len() == 4));
    }

    #[test]
    fn doublet_is_dissolved() {
        // Un vértice extra en medio de la celda central, unido a dos esquinas
        // opuestas: dos quads que comparten dos aristas
        let mut poly = grid(4);
        let cell = poly.faces.remove(5);
        let m = poly.vertices.len() as u32;
        poly.vertices.push(V3::new(1.5, 1.5, 0.0));
        poly.fixed.push(false);
        poly.faces.push(vec![cell[0], cell[1], cell[2], m]);
        poly.faces.push(vec![cell[0], m, cell[2], cell[3]]);
        simplify(&mut poly);
        assert_eq!(energy(&poly), 0);
        assert_eq!(poly.faces.len(), 16);
    }

    #[test]
    fn coincident_vertices_are_merged() {
        // La esquina (2,2) duplicada: la celda de arriba a la derecha usa la copia,
        // unida al original por una arista de largo cero dentro de un pentágono
        let mut poly = grid(4);
        let corner = 2 * 5 + 2;
        let copy = poly.vertices.len() as u32;
        poly.vertices.push(poly.vertices[corner as usize]);
        poly.fixed.push(false);
        // Celda (2,2): [12, 13, 18, 17] → usa la copia; celda (1,2): [11, 12, 17, 16] → pentágono
        let a = poly.faces.iter().position(|f| f == &vec![12, 13, 18, 17]).unwrap();
        poly.faces[a] = vec![copy, 13, 18, 17];
        let b = poly.faces.iter().position(|f| f == &vec![11, 12, 17, 16]).unwrap();
        poly.faces[b] = vec![11, 12, copy, 17, 16];
        let c = poly.faces.iter().position(|f| f == &vec![7, 8, 13, 12]).unwrap();
        poly.faces[c] = vec![7, 8, 13, copy, 12];
        simplify(&mut poly);
        assert_eq!(energy(&poly), 0);
        assert_eq!(poly.faces.len(), 16);
    }

    #[test]
    fn regular_grid_is_untouched() {
        let mut poly = grid(5);
        let before = poly.faces.clone();
        simplify(&mut poly);
        assert_eq!(poly.faces, before);
    }
}
