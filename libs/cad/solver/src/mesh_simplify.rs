//! Quadric Error Metric (QEM) mesh simplification.
//!
//! Port del header Fast-Quadric-Mesh-Simplification de Sven Forstmann (MIT).
//! Reduce el conteo de triángulos mediante edge-collapse iterativo guiado por
//! una métrica de error cuadrático por vértice.

type V3 = [f64; 3];

#[inline]
fn vsub(a: V3, b: V3) -> V3 { [a[0]-b[0], a[1]-b[1], a[2]-b[2]] }
#[inline]
fn vadd(a: V3, b: V3) -> V3 { [a[0]+b[0], a[1]+b[1], a[2]+b[2]] }
#[inline]
fn vscale(a: V3, s: f64) -> V3 { [a[0]*s, a[1]*s, a[2]*s] }
#[inline]
fn vdot(a: V3, b: V3) -> f64 { a[0]*b[0] + a[1]*b[1] + a[2]*b[2] }
#[inline]
fn vcross(a: V3, b: V3) -> V3 {
    [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
}
#[inline]
fn vnorm(a: V3) -> V3 {
    let m = vdot(a, a).sqrt();
    if m > 0.0 { vscale(a, 1.0/m) } else { [0.0, 0.0, 0.0] }
}

/// Matriz simétrica 4x4 almacenada como 10 coeficientes (cuadric).
/// Orden: m11 m12 m13 m14 m22 m23 m24 m33 m34 m44
#[derive(Debug, Clone, Copy)]
struct SymMat([f64; 10]);

impl SymMat {
    fn zero() -> Self { Self([0.0; 10]) }

    /// Cuadric desde plano (a, b, c, d) con a²+b²+c²=1.
    fn from_plane(a: f64, b: f64, c: f64, d: f64) -> Self {
        Self([
            a*a, a*b, a*c, a*d,
            b*b, b*c, b*d,
            c*c, c*d,
            d*d,
        ])
    }

    /// Determinante 3x3 de cualquier submatriz (índices en self.0).
    fn det(&self, a11: usize, a12: usize, a13: usize,
                  a21: usize, a22: usize, a23: usize,
                  a31: usize, a32: usize, a33: usize) -> f64 {
        let m = &self.0;
        m[a11]*m[a22]*m[a33] + m[a13]*m[a21]*m[a32] + m[a12]*m[a23]*m[a31]
        - m[a13]*m[a22]*m[a31] - m[a11]*m[a23]*m[a32] - m[a12]*m[a21]*m[a33]
    }

    /// v^T Q v con v = (x, y, z, 1)
    fn vertex_error(&self, x: f64, y: f64, z: f64) -> f64 {
        let m = &self.0;
        m[0]*x*x + 2.0*m[1]*x*y + 2.0*m[2]*x*z + 2.0*m[3]*x
            + m[4]*y*y + 2.0*m[5]*y*z + 2.0*m[6]*y
            + m[7]*z*z + 2.0*m[8]*z
            + m[9]
    }
}

impl std::ops::Add for SymMat {
    type Output = SymMat;
    fn add(self, rhs: SymMat) -> SymMat {
        let mut o = [0.0; 10];
        for i in 0..10 { o[i] = self.0[i] + rhs.0[i]; }
        SymMat(o)
    }
}
impl std::ops::AddAssign for SymMat {
    fn add_assign(&mut self, rhs: SymMat) {
        for i in 0..10 { self.0[i] += rhs.0[i]; }
    }
}

#[derive(Debug, Clone)]
struct Triangle {
    v: [usize; 3],
    err: [f64; 4],   // err[0..3] = error de colapsar arista i, err[3] = mínimo
    deleted: bool,
    dirty: bool,
    normal: V3,
}

#[derive(Debug, Clone)]
struct Vertex {
    p: V3,
    q: SymMat,
    border: bool,
    tstart: usize,
    tcount: usize,
}

#[derive(Debug, Clone, Copy)]
struct Ref {
    tid: usize,
    tvertex: usize,
}

struct Simplifier {
    verts: Vec<Vertex>,
    tris: Vec<Triangle>,
    refs: Vec<Ref>,
}

impl Simplifier {
    fn new(input_verts: &[V3], input_tris: &[[u32; 3]]) -> Self {
        let verts = input_verts.iter().map(|p| Vertex {
            p: *p,
            q: SymMat::zero(),
            border: false,
            tstart: 0,
            tcount: 0,
        }).collect();

        let tris = input_tris.iter().map(|t| Triangle {
            v: [t[0] as usize, t[1] as usize, t[2] as usize],
            err: [0.0; 4],
            deleted: false,
            dirty: false,
            normal: [0.0; 3],
        }).collect();

        Self { verts, tris, refs: Vec::new() }
    }

    /// Error mínimo al colapsar arista (id_v1 → id_v2). Devuelve (error, posición resultante).
    fn calculate_error(&self, id_v1: usize, id_v2: usize) -> (f64, V3) {
        let q = self.verts[id_v1].q + self.verts[id_v2].q;
        let border = self.verts[id_v1].border && self.verts[id_v2].border;
        let det = q.det(0, 1, 2, 1, 4, 5, 2, 5, 7);

        if det != 0.0 && !border {
            // Solución cerrada: minimizar v^T Q v
            let inv = -1.0 / det;
            let x =  inv * q.det(1, 2, 3, 4, 5, 6, 5, 7, 8);
            let y = -inv * q.det(0, 2, 3, 1, 5, 6, 2, 7, 8);
            let z =  inv * q.det(0, 1, 3, 1, 4, 6, 2, 5, 8);
            let p = [x, y, z];
            let error = q.vertex_error(x, y, z);
            (error, p)
        } else {
            // Caer a evaluar 3 candidatos: p1, p2, midpoint
            let p1 = self.verts[id_v1].p;
            let p2 = self.verts[id_v2].p;
            let p3 = vscale(vadd(p1, p2), 0.5);
            let e1 = q.vertex_error(p1[0], p1[1], p1[2]);
            let e2 = q.vertex_error(p2[0], p2[1], p2[2]);
            let e3 = q.vertex_error(p3[0], p3[1], p3[2]);
            let mut best_e = e1;
            let mut best_p = p1;
            if e2 < best_e { best_e = e2; best_p = p2; }
            if e3 < best_e { best_e = e3; best_p = p3; }
            (best_e, best_p)
        }
    }

    /// Reconstruye refs (mapa vertex → triángulos) y, en la iteración 0, calcula
    /// cuádricas iniciales por vértice + marca bordes.
    fn update_mesh(&mut self, iteration: usize) {
        // Compactar triángulos no borrados (solo desde iter 1+)
        if iteration > 0 {
            let mut dst = 0usize;
            for i in 0..self.tris.len() {
                if !self.tris[i].deleted {
                    if dst != i { self.tris[dst] = self.tris[i].clone(); }
                    dst += 1;
                }
            }
            self.tris.truncate(dst);
        }

        // Cuádricas iniciales + normales (solo iteración 0)
        if iteration == 0 {
            for v in self.verts.iter_mut() { v.q = SymMat::zero(); }
            for t in self.tris.iter_mut() {
                let p0 = self.verts[t.v[0]].p;
                let p1 = self.verts[t.v[1]].p;
                let p2 = self.verts[t.v[2]].p;
                let n = vnorm(vcross(vsub(p1, p0), vsub(p2, p0)));
                t.normal = n;
                let d = -vdot(n, p0);
                let q = SymMat::from_plane(n[0], n[1], n[2], d);
                self.verts[t.v[0]].q += q;
                self.verts[t.v[1]].q += q;
                self.verts[t.v[2]].q += q;
            }
            // Error por arista de cada triángulo (cache para sort)
            for ti in 0..self.tris.len() {
                let mut errs = [0.0f64; 3];
                for j in 0..3 {
                    let a = self.tris[ti].v[j];
                    let b = self.tris[ti].v[(j+1) % 3];
                    errs[j] = self.calculate_error(a, b).0;
                }
                let min = errs[0].min(errs[1]).min(errs[2]);
                self.tris[ti].err = [errs[0], errs[1], errs[2], min];
            }
        }

        // Construir refs
        for v in self.verts.iter_mut() { v.tstart = 0; v.tcount = 0; }
        for t in self.tris.iter() {
            for &vi in &t.v { self.verts[vi].tcount += 1; }
        }
        let mut tstart = 0usize;
        for v in self.verts.iter_mut() {
            v.tstart = tstart;
            tstart += v.tcount;
            v.tcount = 0;
        }
        self.refs.clear();
        self.refs.resize(self.tris.len() * 3, Ref { tid: 0, tvertex: 0 });
        for ti in 0..self.tris.len() {
            for j in 0..3 {
                let v = &mut self.verts[self.tris[ti].v[j]];
                self.refs[v.tstart + v.tcount] = Ref { tid: ti, tvertex: j };
                v.tcount += 1;
            }
        }

        // Identificar bordes (solo iter 0)
        if iteration == 0 {
            // Reset border
            for v in self.verts.iter_mut() { v.border = false; }
            let mut vcount: Vec<usize> = Vec::new();
            let mut vids: Vec<usize> = Vec::new();
            for i in 0..self.verts.len() {
                vcount.clear();
                vids.clear();
                let tstart = self.verts[i].tstart;
                let tcount = self.verts[i].tcount;
                for k in 0..tcount {
                    let r = self.refs[tstart + k];
                    let t = &self.tris[r.tid];
                    for j in 0..3 {
                        let id = t.v[j];
                        if id == i { continue; }
                        let mut found = false;
                        for (idx, &vid) in vids.iter().enumerate() {
                            if vid == id { vcount[idx] += 1; found = true; break; }
                        }
                        if !found { vids.push(id); vcount.push(1); }
                    }
                }
                for (idx, &c) in vcount.iter().enumerate() {
                    if c == 1 {
                        self.verts[vids[idx]].border = true;
                    }
                }
            }
            // Recalcular cuádricas con peso extra por bordes (Forstmann hace esto sumando un plano perpendicular).
            // Versión simplificada: marcamos pero no agregamos extra penalty.
            // En la práctica el chequeo "border && border" en calculate_error ya protege bordes.
        }
    }

    /// ¿Colapsar i0→i1 con destino p genera triángulos invertidos (flip)?
    fn flipped(&self, p: V3, i0: usize, i1: usize, deleted: &mut Vec<bool>) -> bool {
        let tcount = self.verts[i0].tcount;
        let tstart = self.verts[i0].tstart;
        deleted.clear();
        deleted.resize(tcount, false);

        for k in 0..tcount {
            let r = self.refs[tstart + k];
            let t = &self.tris[r.tid];
            if t.deleted { continue; }
            let s = r.tvertex;
            let id1 = t.v[(s+1) % 3];
            let id2 = t.v[(s+2) % 3];

            if id1 == i1 || id2 == i1 {
                deleted[k] = true;
                continue;
            }
            let d1 = vnorm(vsub(self.verts[id1].p, p));
            let d2 = vnorm(vsub(self.verts[id2].p, p));
            if vdot(d1, d2).abs() > 0.999 { return true; }
            let n = vnorm(vcross(d1, d2));
            deleted[k] = false;
            if vdot(n, t.normal) < 0.2 { return true; }
        }
        false
    }

    /// Actualizar triángulos que comparten vértice i0 después de colapso: reapuntar
    /// al nuevo vértice y recalcular cuádrica/errores. Devuelve total de tris marcados borrados.
    fn update_triangles(&mut self, i0: usize, vi: usize, deleted: &[bool], deleted_triangles: &mut usize) {
        let tcount = self.verts[vi].tcount;
        let tstart = self.verts[vi].tstart;
        for k in 0..tcount {
            let r = self.refs[tstart + k];
            let tid = r.tid;
            if self.tris[tid].deleted { continue; }
            if deleted[k] {
                self.tris[tid].deleted = true;
                *deleted_triangles += 1;
                continue;
            }
            self.tris[tid].v[r.tvertex] = i0;
            self.tris[tid].dirty = true;
            let v0 = self.tris[tid].v[0];
            let v1 = self.tris[tid].v[1];
            let v2 = self.tris[tid].v[2];
            let e0 = self.calculate_error(v0, v1).0;
            let e1 = self.calculate_error(v1, v2).0;
            let e2 = self.calculate_error(v2, v0).0;
            let min = e0.min(e1).min(e2);
            self.tris[tid].err = [e0, e1, e2, min];
            // Append ref para el nuevo vértice i0
            self.refs.push(Ref { tid, tvertex: r.tvertex });
        }
    }

    fn simplify(&mut self, target_count: usize, aggressiveness: f64, max_iter: usize) {
        // Reset deleted flags
        for t in self.tris.iter_mut() { t.deleted = false; }

        let mut deleted_triangles: usize = 0;
        let initial_total = self.tris.len();
        let mut deleted0: Vec<bool> = Vec::new();
        let mut deleted1: Vec<bool> = Vec::new();

        for iteration in 0..max_iter {
            let remaining = initial_total - deleted_triangles;
            if remaining <= target_count { break; }

            // Cada 5 iter compactamos + reconstruimos refs
            if iteration % 5 == 0 {
                self.update_mesh(iteration);
                deleted_triangles = 0;
            }

            // Limpiar flag dirty
            for t in self.tris.iter_mut() { t.dirty = false; }

            // Umbral de error: crece con iteración (más agresivo)
            let threshold = 0.000_000_001_f64 * ((iteration as f64) + 3.0).powf(aggressiveness);

            for ti in 0..self.tris.len() {
                if self.tris[ti].err[3] > threshold { continue; }
                if self.tris[ti].deleted { continue; }
                if self.tris[ti].dirty { continue; }

                for j in 0..3 {
                    if self.tris[ti].err[j] >= threshold { continue; }
                    let i0 = self.tris[ti].v[j];
                    let i1 = self.tris[ti].v[(j+1) % 3];

                    // No colapsar entre borde y no-borde
                    if self.verts[i0].border != self.verts[i1].border { continue; }

                    // Posición resultante del colapso
                    let (_err, p) = self.calculate_error(i0, i1);

                    deleted0.resize(self.verts[i0].tcount, false);
                    deleted1.resize(self.verts[i1].tcount, false);

                    if self.flipped(p, i0, i1, &mut deleted0) { continue; }
                    if self.flipped(p, i1, i0, &mut deleted1) { continue; }

                    // OK, colapsar: mover i0 a p, sumar cuádrica de i1 a i0
                    self.verts[i0].p = p;
                    self.verts[i0].q = self.verts[i0].q + self.verts[i1].q;

                    let tstart_new = self.refs.len();

                    self.update_triangles(i0, i0, &deleted0, &mut deleted_triangles);
                    self.update_triangles(i0, i1, &deleted1, &mut deleted_triangles);

                    let tcount_new = self.refs.len() - tstart_new;
                    if tcount_new <= self.verts[i0].tcount {
                        // Reusar slot existente
                        if tcount_new > 0 {
                            for k in 0..tcount_new {
                                self.refs[self.verts[i0].tstart + k] = self.refs[tstart_new + k];
                            }
                        }
                    } else {
                        self.verts[i0].tstart = tstart_new;
                    }
                    self.verts[i0].tcount = tcount_new;

                    if initial_total - deleted_triangles <= target_count { break; }
                    break; // siguiente triángulo
                }
                if initial_total - deleted_triangles <= target_count { break; }
            }
        }

        self.compact_result();
    }

    /// Eliminar tris borrados y re-mapear vértices no usados.
    fn compact_result(&mut self) {
        // Compactar tris
        let mut dst = 0usize;
        for i in 0..self.tris.len() {
            if !self.tris[i].deleted {
                if dst != i { self.tris[dst] = self.tris[i].clone(); }
                dst += 1;
            }
        }
        self.tris.truncate(dst);

        // Marcar vértices usados
        let n = self.verts.len();
        let mut used = vec![false; n];
        for t in self.tris.iter() {
            used[t.v[0]] = true;
            used[t.v[1]] = true;
            used[t.v[2]] = true;
        }
        // Mapa old → new
        let mut remap = vec![usize::MAX; n];
        let mut new_verts: Vec<Vertex> = Vec::new();
        for i in 0..n {
            if used[i] {
                remap[i] = new_verts.len();
                new_verts.push(self.verts[i].clone());
            }
        }
        for t in self.tris.iter_mut() {
            t.v[0] = remap[t.v[0]];
            t.v[1] = remap[t.v[1]];
            t.v[2] = remap[t.v[2]];
        }
        self.verts = new_verts;
    }

    fn into_arrays(self) -> (Vec<V3>, Vec<[u32; 3]>) {
        let v = self.verts.iter().map(|v| v.p).collect();
        let t = self.tris.iter().map(|t| [t.v[0] as u32, t.v[1] as u32, t.v[2] as u32]).collect();
        (v, t)
    }
}

/// Simplificar malla triangular a `target_count` triángulos.
///
/// `aggressiveness` controla la velocidad de crecimiento del umbral de error (5–8 típico).
/// Valores más altos colapsan más rápido pero introducen más distorsión.
pub fn simplify(
    verts: &[V3],
    tris: &[[u32; 3]],
    target_count: usize,
    aggressiveness: f64,
) -> (Vec<V3>, Vec<[u32; 3]>) {
    if tris.len() <= target_count || verts.is_empty() {
        return (verts.to_vec(), tris.to_vec());
    }
    let mut s = Simplifier::new(verts, tris);
    s.simplify(target_count, aggressiveness, 100);
    s.into_arrays()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cubo subdividido: 6 caras × N×N quads × 2 tris.
    fn make_subdivided_cube(n: usize) -> (Vec<V3>, Vec<[u32; 3]>) {
        let mut verts: Vec<V3> = Vec::new();
        let mut tris: Vec<[u32; 3]> = Vec::new();
        // 6 caras del cubo [-1,1]^3
        let faces = [
            // (axis_fixed, value, axis_u, axis_v, flip)
            (0,  1.0, 1, 2, false),
            (0, -1.0, 1, 2, true),
            (1,  1.0, 0, 2, true),
            (1, -1.0, 0, 2, false),
            (2,  1.0, 0, 1, false),
            (2, -1.0, 0, 1, true),
        ];
        for (af, val, au, av, flip) in faces {
            let base = verts.len() as u32;
            for j in 0..=n {
                for i in 0..=n {
                    let u = -1.0 + 2.0 * (i as f64) / (n as f64);
                    let v = -1.0 + 2.0 * (j as f64) / (n as f64);
                    let mut p = [0.0; 3];
                    p[af] = val;
                    p[au] = u;
                    p[av] = v;
                    verts.push(p);
                }
            }
            let stride = (n + 1) as u32;
            for j in 0..n as u32 {
                for i in 0..n as u32 {
                    let a = base + j*stride + i;
                    let b = a + 1;
                    let c = a + stride;
                    let d = c + 1;
                    if flip {
                        tris.push([a, c, b]);
                        tris.push([b, c, d]);
                    } else {
                        tris.push([a, b, c]);
                        tris.push([b, d, c]);
                    }
                }
            }
        }
        (verts, tris)
    }

    #[test]
    fn cube_simplifies_to_target() {
        let (verts, tris) = make_subdivided_cube(8);
        let initial = tris.len();
        let target = initial / 4;
        let (_v, t) = simplify(&verts, &tris, target, 7.0);
        assert!(t.len() <= initial, "no debe crecer");
        assert!(t.len() <= (target as f64 * 1.5) as usize,
            "esperado ≈{target}, obtenido {}", t.len());
    }

    #[test]
    fn cube_no_nan() {
        let (verts, tris) = make_subdivided_cube(6);
        let (v, _t) = simplify(&verts, &tris, 50, 7.0);
        for p in v.iter() {
            for c in p { assert!(c.is_finite(), "NaN/Inf en resultado"); }
        }
    }

    #[test]
    fn target_below_minimum_terminates() {
        // Pedir target imposible (4 tris) — no debe colgar
        let (verts, tris) = make_subdivided_cube(4);
        let (_v, t) = simplify(&verts, &tris, 4, 7.0);
        assert!(!t.is_empty());
    }

    #[test]
    fn already_below_target_passthrough() {
        let verts: Vec<V3> = vec![[0.0,0.0,0.0],[1.0,0.0,0.0],[0.0,1.0,0.0]];
        let tris = vec![[0u32, 1, 2]];
        let (v, t) = simplify(&verts, &tris, 100, 7.0);
        assert_eq!(v.len(), 3);
        assert_eq!(t.len(), 1);
    }
}
