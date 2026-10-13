//! Resta de una zanja (prisma convexo infinito con tres caras) a una malla cerrada
//!
//! Sirve para las ranuras deslizantes: la zanja entra por una cara plana y sale por la
//! pared de la pieza en los dos extremos, así que hay que recortar triángulos cualesquiera,
//! no solo la tapa. Pasos:
//!
//! 1. Cada triángulo que toca la zanja se recorta contra sus tres planos y se queda con
//!    lo de afuera (polígonos convexos).
//! 2. Los puntos nuevos sobre las aristas originales se agregan también a los vecinos
//!    (si no, quedan uniones en T).
//! 3. El borde abierto que deja el recorte es un solo contorno; se parte en tramos por el
//!    plano de la zanja en que cae, y cada cara de la zanja se cierra con los triángulos
//!    de su tramo más la arista que comparte con la cara vecina.

use std::collections::HashMap;

use pinocchio_math::{Real, Vector3};
use pinocchio_mesh::Mesh;

use crate::contact::{clip_half_plane, polygon_area};
use crate::error::{Print3dError, Result};
use crate::geom::{diagonal, triangles, weld_ids};

/// Plano de la zanja: un punto y la normal que mira hacia el interior de la zanja (el vacío)
pub(crate) type TrenchPlane = (Vector3, Vector3);

fn err(m: &str) -> Print3dError {
    Print3dError::JointError(m.to_string())
}

/// Planos de una zanja de cola de milano: boca de semiancho `a` en la superficie (de normal exterior
/// `m`, pasando por `center`), semiancho `b > a` al fondo a `d` de hondo, corriendo a lo largo de `s`
pub(crate) fn dovetail_trench(center: Vector3, m: Vector3, s: Vector3, a: Real, b: Real, d: Real) -> [TrenchPlane; 3] {
    let v = m.cross(&s).normalize();
    let down = -m;
    [
        (center + v * a, (v * -d + down * (b - a)).normalize()),
        (center - v * a, (v * d + down * (b - a)).normalize()),
        (center + down * d, m),
    ]
}

/// Resta la zanja `planes` = [lado 1, lado 2, fondo] de `mesh`. Falla si el contorno que deja
/// no es el de una zanja que cruza la pieza de pared a pared
pub(crate) fn subtract_trench(mesh: &Mesh, planes: &[TrenchPlane; 3]) -> Result<Mesh> {
    let (pos, tris) = triangles(mesh);
    if tris.is_empty() {
        return Err(Print3dError::EmptyMesh);
    }
    let diag = diagonal(&pos);
    let weld_tol = (diag * 1e-9).max(1e-9);
    let plane_tol = (diag * 1e-6).max(1e-7);
    let area_tol = (diag * diag * 1e-12).max(1e-12);
    let wid = weld_ids(&pos, weld_tol);
    let side = |i: usize, p: &Vector3| planes[i].1.dot(&(*p - planes[i].0));

    // Paso 1: recorte
    struct Item {
        tri: [usize; 3],
        /// Polígonos que quedan afuera de la zanja (vacío: el triángulo no se toca)
        polys: Vec<Vec<Vector3>>,
        clipped: bool,
    }
    let mut items: Vec<Item> = Vec::with_capacity(tris.len());
    for t in &tris {
        let poly = vec![pos[t[0]], pos[t[1]], pos[t[2]]];
        // ¿Entra a la zanja con área?
        let mut inside = poly.clone();
        for i in 0..3 {
            inside = clip_half_plane(&inside, |p| side(i, p));
            if inside.len() < 3 {
                break;
            }
        }
        let n = (poly[1] - poly[0]).cross(&(poly[2] - poly[0]));
        let touches = inside.len() >= 3 && polygon_area(&inside, n.try_normalize().unwrap_or_else(Vector3::unit_z)) > area_tol;
        if !touches {
            items.push(Item { tri: *t, polys: vec![poly], clipped: false });
            continue;
        }
        let mut rest = poly;
        let mut outs = Vec::new();
        for i in 0..3 {
            let out = clip_half_plane(&rest, |p| -side(i, p));
            if out.len() >= 3 && polygon_area(&out, n.try_normalize().unwrap_or_else(Vector3::unit_z)) > area_tol {
                outs.push(out);
            }
            rest = clip_half_plane(&rest, |p| side(i, p));
            if rest.len() < 3 {
                break;
            }
        }
        items.push(Item { tri: *t, polys: outs, clipped: true });
    }

    // Paso 2: puntos sobre aristas originales (clave: vértices soldados ordenados)
    let ekey = |a: usize, b: usize| (wid[a].min(wid[b]), wid[a].max(wid[b]));
    let on_edge = |p: &Vector3, u: &Vector3, v: &Vector3| -> Option<Real> {
        let d = *v - *u;
        let len2 = d.length_squared();
        if len2 <= 0.0 {
            return None;
        }
        let t = (*p - *u).dot(&d) / len2;
        let dist = (*p - (*u + d * t)).length();
        (t > 1e-9 && t < 1.0 - 1e-9 && dist <= plane_tol).then_some(t)
    };
    let mut edge_pts: HashMap<(usize, usize), Vec<(Real, Vector3)>> = HashMap::new();
    for it in items.iter().filter(|i| i.clipped) {
        for k in 0..3 {
            let (a, b) = (it.tri[k], it.tri[(k + 1) % 3]);
            let (u, v) = (pos[a], pos[b]);
            // El orden de los parámetros es respecto de la arista ordenada
            let (lo, hi) = if wid[a] <= wid[b] { (u, v) } else { (v, u) };
            for poly in &it.polys {
                for p in poly {
                    if let Some(t) = on_edge(p, &lo, &hi) {
                        let list = edge_pts.entry(ekey(a, b)).or_default();
                        if !list.iter().any(|(s, _)| (s - t).abs() < 1e-9) {
                            list.push((t, *p));
                        }
                    }
                }
            }
        }
    }

    // Paso 3: triángulos finales (polígonos con los puntos del vecino insertados, en abanico desde el centro)
    let mut out_pos: Vec<Vector3> = Vec::new();
    let mut out_tris: Vec<[usize; 3]> = Vec::new();
    for it in &items {
        for poly in &it.polys {
            // Insertar en cada arista del polígono los puntos de la arista original donde cae
            let mut ring: Vec<Vector3> = Vec::new();
            for i in 0..poly.len() {
                let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
                ring.push(p);
                for k in 0..3 {
                    let (a, b) = (it.tri[k], it.tri[(k + 1) % 3]);
                    let Some(list) = edge_pts.get(&ekey(a, b)) else { continue };
                    let (lo, hi) = if wid[a] <= wid[b] { (pos[a], pos[b]) } else { (pos[b], pos[a]) };
                    // p y q sobre la misma arista original (incluidos sus extremos)
                    let near = |x: &Vector3| {
                        let d = hi - lo;
                        let t = (*x - lo).dot(&d) / d.length_squared().max(1e-30);
                        (*x - (lo + d * t)).length() <= plane_tol
                    };
                    if !(near(&p) && near(&q)) {
                        continue;
                    }
                    let d = hi - lo;
                    let param = |x: &Vector3| (*x - lo).dot(&d) / d.length_squared().max(1e-30);
                    let (tp, tq) = (param(&p), param(&q));
                    let mut between: Vec<(Real, Vector3)> = list.iter().filter(|(t, _)| *t > tp.min(tq) + 1e-9 && *t < tp.max(tq) - 1e-9).copied().collect();
                    between.sort_by(|a, b| if tp < tq { a.0.total_cmp(&b.0) } else { b.0.total_cmp(&a.0) });
                    ring.extend(between.into_iter().map(|(_, x)| x));
                }
            }
            if ring.len() < 3 {
                continue;
            }
            let base = out_pos.len();
            let m = ring.len();
            if m == 3 {
                out_pos.extend(ring);
                out_tris.push([base, base + 1, base + 2]);
            } else {
                let c = ring.iter().fold(Vector3::zero(), |s, p| s + *p) * (1.0 / m as Real);
                out_pos.extend(ring);
                out_pos.push(c);
                for i in 0..m {
                    out_tris.push([base + i, base + (i + 1) % m, base + m]);
                }
            }
        }
    }

    // Soldar
    let weld = weld_ids(&out_pos, weld_tol);
    let mut rep: HashMap<usize, Vector3> = HashMap::new();
    for (i, &w) in weld.iter().enumerate() {
        rep.entry(w).or_insert(out_pos[i]);
    }
    let mut positions: Vec<Vector3> = (0..rep.len()).map(|w| rep[&w]).collect();
    let mut faces: Vec<[usize; 3]> = out_tris.iter().map(|t| [weld[t[0]], weld[t[1]], weld[t[2]]]).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();

    // Contorno abierto
    let mut directed: HashMap<(usize, usize), u32> = HashMap::new();
    for t in &faces {
        for i in 0..3 {
            *directed.entry((t[i], t[(i + 1) % 3])).or_default() += 1;
        }
    }
    let boundary: Vec<(usize, usize)> = directed.keys().filter(|&&(a, b)| !directed.contains_key(&(b, a))).copied().collect();
    if boundary.is_empty() {
        return Err(err("la zanja no tocó la pieza"));
    }
    let next: HashMap<usize, usize> = boundary.iter().copied().collect();
    if next.len() != boundary.len() {
        return Err(err("el contorno de la zanja no es simple"));
    }
    let start = boundary[0].0;
    let mut lp = vec![start];
    let mut cur = next[&start];
    while cur != start {
        if lp.len() > boundary.len() {
            return Err(err("el contorno de la zanja no se cierra"));
        }
        lp.push(cur);
        cur = *next.get(&cur).ok_or_else(|| err("el contorno de la zanja está roto"))?;
    }
    if lp.len() != boundary.len() {
        return Err(err("la zanja abrió más de un contorno: tiene que cruzar la pieza de pared a pared"));
    }

    // Plano de cada arista del contorno (la que cae en uno solo)
    let on_plane = |p: &Vector3, i: usize| side(i, p).abs() <= plane_tol;
    let label = |a: usize, b: usize| -> Option<usize> {
        let hits: Vec<usize> = (0..3).filter(|&i| on_plane(&positions[a], i) && on_plane(&positions[b], i)).collect();
        (hits.len() == 1).then(|| hits[0])
    };
    let n = lp.len();
    let labels: Vec<Option<usize>> = (0..n).map(|i| label(lp[i], lp[(i + 1) % n])).collect();
    if labels.iter().any(|l| l.is_none()) {
        return Err(err("una arista del contorno de la zanja no cae en ninguno de sus planos"));
    }
    let labels: Vec<usize> = labels.into_iter().flatten().collect();
    // Rotar para que el recorrido empiece en un cambio de plano
    let shift = (0..n).find(|&i| labels[i] != labels[(i + n - 1) % n]).ok_or_else(|| err("el contorno de la zanja está en un solo plano"))?;
    let order: Vec<usize> = (0..n).map(|k| (shift + k) % n).collect();
    // Tramos: (plano, índices de vértices del tramo en orden, de un cambio al siguiente)
    let mut runs: Vec<(usize, Vec<usize>)> = Vec::new();
    for &i in &order {
        match runs.last_mut() {
            Some((l, vs)) if *l == labels[i] => vs.push(lp[(i + 1) % n]),
            _ => runs.push((labels[i], vec![lp[i], lp[(i + 1) % n]])),
        }
    }
    if runs.len() != 4 || runs.iter().enumerate().any(|(k, r)| (r.0 == 2) != (k % 2 == (if runs[0].0 == 2 { 0 } else { 1 }))) {
        return Err(err("el contorno de la zanja no es el de una ranura que cruza la pieza (lado, fondo, lado, fondo)"));
    }
    // Poner un tramo de fondo primero para fijar el orden: [fondo, lado, fondo, lado]
    if runs[0].0 != 2 {
        runs.rotate_left(1);
    }
    let (floor_a, side_a, floor_b, side_b) = (&runs[0], &runs[1], &runs[2], &runs[3]);
    // Caras: cada lado cierra su tramo con el segmento entre sus extremos; el fondo une sus dos tramos
    let first = |r: &(usize, Vec<usize>)| *r.1.first().expect("tramo");
    let last = |r: &(usize, Vec<usize>)| *r.1.last().expect("tramo");
    let mut face_loops: Vec<(usize, Vec<usize>)> = vec![
        (side_a.0, side_a.1.clone()),
        (side_b.0, side_b.1.clone()),
    ];
    // Fondo: tramo A, salto al inicio del lado de B... el recorrido es floor_a → side_a → floor_b → side_b:
    // fondo = floor_a, luego el segmento hasta el inicio de floor_b, floor_b, y el segmento de vuelta
    let mut floor_loop: Vec<usize> = floor_a.1.clone();
    floor_loop.extend(floor_b.1.iter().copied());
    face_loops.push((floor_a.0, floor_loop));
    debug_assert_eq!(last(floor_a), first(side_a));
    debug_assert_eq!(last(side_a), first(floor_b));

    // Triangular cada cara con su normal hacia el vacío
    for (plane, vs) in &face_loops {
        let segs: Vec<(Vector3, Vector3)> = (0..vs.len()).map(|k| (positions[vs[k]], positions[vs[(k + 1) % vs.len()]])).collect();
        let tris3 = crate::capping::cap_triangles(&segs, planes[*plane].1, planes[*plane].0, weld_tol);
        if tris3.is_empty() {
            return Err(err("no se pudo cerrar una cara de la zanja"));
        }
        for t in tris3 {
            let base = positions.len();
            positions.extend(t);
            faces.push([base, base + 1, base + 2]);
        }
    }

    // Resultado soldado
    let weld2 = weld_ids(&positions, weld_tol);
    let mut rep2: HashMap<usize, Vector3> = HashMap::new();
    for (i, &w) in weld2.iter().enumerate() {
        rep2.entry(w).or_insert(positions[i]);
    }
    let final_pos: Vec<Vector3> = (0..rep2.len()).map(|w| rep2[&w]).collect();
    let final_faces: Vec<[usize; 3]> = faces.iter().map(|t| [weld2[t[0]], weld2[t[1]], weld2[t[2]]]).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();
    let mut used = vec![false; final_pos.len()];
    for t in &final_faces {
        for &i in t {
            used[i] = true;
        }
    }
    let mut remap = vec![usize::MAX; final_pos.len()];
    let mut compact = Vec::new();
    for (i, p) in final_pos.iter().enumerate() {
        if used[i] {
            remap[i] = compact.len();
            compact.push(*p);
        }
    }
    let compact_faces: Vec<[usize; 3]> = final_faces.iter().map(|t| [remap[t[0]], remap[t[1]], remap[t[2]]]).collect();
    let result = Mesh::from_triangles(&compact, &compact_faces);
    if !result.is_closed() {
        return Err(err("la malla con la ranura no quedó cerrada"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::compute_volume;
    use crate::printability::tests::make_box;

    /// Zanja trapezoidal a lo largo de X en una caja [0,80]x[0,60]x[0,40] por su cara de arriba (z = 40)
    fn trench(a: Real, b: Real, d: Real, y0: Real) -> [TrenchPlane; 3] {
        // Perfil en (y, profundidad): boca de semiancho a en la superficie y b al fondo
        let top = 40.0;
        let norm = |dy: Real, dd: Real| {
            // dd es profundidad (hacia −z): el vector 3D es (0, dy, −dd)
            Vector3::new(0.0, dy, -dd).normalize()
        };
        let n1 = norm(-d, b - a); // lado +y: hacia −y y hacia abajo
        let n2 = norm(d, b - a); // lado −y: hacia +y y hacia abajo
        [
            (Vector3::new(0.0, y0 + a, top), n1),
            (Vector3::new(0.0, y0 - a, top), n2),
            (Vector3::new(0.0, y0, top - d), Vector3::new(0.0, 0.0, 1.0)),
        ]
    }

    #[test]
    fn cola_de_milano_cruza_la_caja() {
        let b = make_box(80.0, 60.0, 40.0);
        let (a, bb, d) = (3.0, 5.0, 6.0);
        let planes = trench(a, bb, d, 30.0);
        let r = subtract_trench(&b, &planes).unwrap();
        assert!(r.is_closed());
        // Sección del trapecio por el largo de la caja (80): ((2a + 2b)/2 · d)
        let expected = 64000.0 * 1.5 - 0.0; // placeholder reemplazado abajo
        let _ = expected;
        let area = (2.0 * a + 2.0 * bb) * 0.5 * d;
        let vol = compute_volume(&b) - compute_volume(&r);
        assert!((vol - area * 80.0).abs() < 1e-6 * vol, "{vol} vs {}", area * 80.0);
    }

    #[test]
    fn zanja_que_no_cruza_falla() {
        let b = make_box(80.0, 60.0, 40.0);
        // La zanja entra por una cara lateral (x = 0) pero se queda adentro? Se pide fuera de la caja
        let planes = trench(3.0, 5.0, 6.0, 500.0);
        assert!(subtract_trench(&b, &planes).is_err());
    }

    fn sphere(r: Real, rings: usize, segs: usize) -> Mesh {
        let mut pos = vec![Vector3::new(0.0, 0.0, r)];
        for i in 1..rings {
            let th = std::f64::consts::PI * i as Real / rings as Real;
            for j in 0..segs {
                let ph = std::f64::consts::TAU * j as Real / segs as Real;
                pos.push(Vector3::new(r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos()));
            }
        }
        pos.push(Vector3::new(0.0, 0.0, -r));
        let bottom = pos.len() - 1;
        let at = |i: usize, j: usize| 1 + (i - 1) * segs + j % segs;
        let mut tris = Vec::new();
        for j in 0..segs {
            tris.push([0, at(1, j), at(1, j + 1)]);
            tris.push([bottom, at(rings - 1, j + 1), at(rings - 1, j)]);
        }
        for i in 1..rings - 1 {
            for j in 0..segs {
                tris.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        Mesh::from_triangles(&pos, &tris)
    }

    /// Mitad de abajo de una esfera cortada en z = 5 (pared curva)
    fn lower_cap() -> Mesh {
        use crate::slicer::{slice_by_plane, Plane};
        let s = sphere(40.0, 48, 64);
        let (x, y) = slice_by_plane(&s, &Plane::xy_at_z(5.0)).unwrap();
        let zmax = |m: &Mesh| m.vertices.iter().map(|v| v.position.z()).fold(Real::NEG_INFINITY, Real::max);
        if zmax(&x) < zmax(&y) { x } else { y }
    }

    #[test]
    fn ranura_en_una_pared_curva() {
        let m = lower_cap();
        assert!(m.is_closed());
        let planes = dovetail_trench(Vector3::new(0.0, 0.0, 5.0), Vector3::unit_z(), Vector3::unit_x(), 3.0, 5.0, 6.0);
        let r = subtract_trench(&m, &planes).unwrap();
        assert!(r.is_closed());
        let removed = compute_volume(&m) - compute_volume(&r);
        // Sección ((2·3 + 2·5)/2)·6 = 48 mm² por la cuerda de la esfera entre z = 5 y −1 (≈ 79,5 mm)
        assert!(removed > 48.0 * 78.0 && removed < 48.0 * 81.0, "{removed}");
    }

    #[test]
    fn ranura_oblicua_en_la_caja() {
        let b = make_box(80.0, 60.0, 40.0);
        let s = Vector3::new(1.0, 0.6, 0.0).normalize();
        let planes = dovetail_trench(Vector3::new(40.0, 30.0, 40.0), Vector3::unit_z(), s, 2.5, 4.0, 5.0);
        let r = subtract_trench(&b, &planes).unwrap();
        assert!(r.is_closed());
        let removed = compute_volume(&b) - compute_volume(&r);
        // Sección 2·(2.5+4)/2·5 = 32.5 mm² por la longitud de la cuerda
        assert!(removed > 32.5 * 40.0 && removed < 32.5 * 100.0, "{removed}");
    }
}
