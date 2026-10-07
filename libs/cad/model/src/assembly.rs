//! Ensambles: instancias de las piezas del diseño, cada una con su posición y
//! giro, unidas por relaciones entre conectores (sistemas de coordenadas
//! sobre las piezas), como los *mates* de Onshape. El solver mueve las
//! instancias libres (6 incógnitas cada una) hasta cumplir las relaciones.

use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};

use crate::eval::Evaluation;
use crate::feature::PartId;
use crate::geom::*;
use cad_occt::Shape;

/// Ensamble guardado en el documento.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Assembly {
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub mates: Vec<Mate>,
    #[serde(default)]
    pub next_id: u32,
}

/// Una pieza del diseño puesta en el ensamble.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: u32,
    pub part: PartId,
    pub name: String,
    /// Traslación (mm) y giro como vector (eje × ángulo en radianes), aplicados a la pieza.
    #[serde(default)]
    pub position: P3,
    #[serde(default)]
    pub rotation: P3,
    /// Fija: el solver no la mueve.
    #[serde(default)]
    pub fixed: bool,
}

/// Sistema de coordenadas sobre una instancia, en coordenadas de la pieza
/// (sin la posición de la instancia): origen, eje Z y eje X.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Connector {
    pub instance: u32,
    pub origin: P3,
    pub z: P3,
    pub x: P3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MateKind {
    /// Pegadas: 0 grados libres.
    Fastened,
    /// Bisagra: gira alrededor del eje Z común.
    Revolute,
    /// Desliza a lo largo del eje Z común, sin girar.
    Slider,
    /// Gira y desliza sobre el eje Z común.
    Cylindrical,
    /// Apoyadas en el plano XY: se mueven en él y giran alrededor de Z.
    Planar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mate {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub kind: MateKind,
    pub a: Connector,
    pub b: Connector,
    /// Z de `b` al revés (caras enfrentadas).
    #[serde(default)]
    pub flip: bool,
    /// Ángulo impuesto (grados) en revolutas y cilíndricas: para animar o posar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    /// Distancia impuesta (mm) a lo largo de Z en deslizantes y cilíndricas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
}

/// Matriz de giro de un vector de giro (Rodrigues).
pub fn rotation_matrix(r: P3) -> [[f64; 3]; 3] {
    let th = norm(r);
    if th < 1e-12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let [x, y, z] = scale(r, 1.0 / th);
    let (s, c) = th.sin_cos();
    let t = 1.0 - c;
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ]
}

fn mul(m: &[[f64; 3]; 3], v: P3) -> P3 {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Matriz 3×4 (giro y traslación) de una instancia.
pub fn instance_matrix(position: P3, rotation: P3) -> [[f64; 4]; 3] {
    let r = rotation_matrix(rotation);
    [0, 1, 2].map(|i| [r[i][0], r[i][1], r[i][2], position[i]])
}

/// Conector en el mundo: origen, Z y X.
fn world(c: &Connector, pos: P3, rot: P3) -> (P3, P3, P3) {
    let r = rotation_matrix(rot);
    (add(mul(&r, c.origin), pos), mul(&r, normalize(c.z)), mul(&r, normalize(c.x)))
}

/// Ecuaciones de una relación (cero cuando se cumple).
fn residuals(m: &Mate, a: (P3, P3, P3), b: (P3, P3, P3), out: &mut Vec<f64>) {
    let (oa, za, xa) = a;
    let (ob, zb, xb) = b;
    let zb = if m.flip { scale(zb, -1.0) } else { zb };
    let d = sub(ob, oa);
    // Componente de d perpendicular al eje común
    let perp = sub(d, scale(za, dot(d, za)));
    match m.kind {
        MateKind::Fastened => {
            out.extend_from_slice(&d);
            out.extend_from_slice(&sub(zb, za));
            out.extend_from_slice(&sub(xb, xa));
        }
        MateKind::Revolute | MateKind::Cylindrical => {
            if m.kind == MateKind::Revolute {
                out.extend_from_slice(&d);
            } else {
                out.extend_from_slice(&perp);
                if let Some(dist) = m.distance {
                    out.push(dot(d, za) - dist);
                }
            }
            out.extend_from_slice(&sub(zb, za));
            if let Some(angle) = m.angle {
                // X de b = X de a girado `angle` alrededor de Z
                let (s, c) = angle.to_radians().sin_cos();
                let want = add(scale(xa, c), scale(cross(za, xa), s));
                out.extend_from_slice(&sub(xb, want));
            }
        }
        MateKind::Slider => {
            out.extend_from_slice(&perp);
            out.extend_from_slice(&sub(zb, za));
            out.extend_from_slice(&sub(xb, xa));
            if let Some(dist) = m.distance {
                out.push(dot(d, za) - dist);
            }
        }
        MateKind::Planar => {
            out.push(dot(d, za));
            out.extend_from_slice(&sub(zb, za));
        }
    }
}

/// Resultado del solver.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssemblySolution {
    /// Posición y giro de cada instancia (en el orden del ensamble).
    pub poses: Vec<(u32, P3, P3)>,
    /// Norma de los residuos al terminar (0 = todas las relaciones se cumplen).
    pub residual: f64,
    pub converged: bool,
    /// Grados de libertad que quedan (incógnitas menos el rango del sistema).
    pub dof: usize,
}

/// Mueve las instancias libres para cumplir las relaciones (Levenberg-Marquardt
/// con jacobiano numérico). Si ninguna está fija, la primera queda quieta.
pub fn solve(asm: &Assembly) -> AssemblySolution {
    let anchor = if asm.instances.iter().any(|i| i.fixed) { None } else { asm.instances.first().map(|i| i.id) };
    let free: Vec<usize> = asm.instances.iter().enumerate().filter(|(_, i)| !i.fixed && Some(i.id) != anchor).map(|(k, _)| k).collect();
    let mut x = DVector::<f64>::zeros(free.len() * 6);
    for (j, &k) in free.iter().enumerate() {
        let i = &asm.instances[k];
        for c in 0..3 {
            x[j * 6 + c] = i.position[c];
            x[j * 6 + 3 + c] = i.rotation[c];
        }
    }
    let pose = |x: &DVector<f64>, id: u32| -> Option<(P3, P3)> {
        let k = asm.instances.iter().position(|i| i.id == id)?;
        match free.iter().position(|&f| f == k) {
            Some(j) => Some(([x[j * 6], x[j * 6 + 1], x[j * 6 + 2]], [x[j * 6 + 3], x[j * 6 + 4], x[j * 6 + 5]])),
            None => Some((asm.instances[k].position, asm.instances[k].rotation)),
        }
    };
    let eval = |x: &DVector<f64>| -> DVector<f64> {
        let mut r = Vec::new();
        for m in &asm.mates {
            let (Some((pa, ra)), Some((pb, rb))) = (pose(x, m.a.instance), pose(x, m.b.instance)) else { continue };
            residuals(m, world(&m.a, pa, ra), world(&m.b, pb, rb), &mut r);
        }
        DVector::from_vec(r)
    };
    let jac = |x: &DVector<f64>, f0: &DVector<f64>| -> DMatrix<f64> {
        let mut j = DMatrix::<f64>::zeros(f0.len(), x.len());
        for c in 0..x.len() {
            let h = 1e-7 * (1.0 + x[c].abs());
            let mut xp = x.clone();
            xp[c] += h;
            let mut xm = x.clone();
            xm[c] -= h;
            let col = (eval(&xp) - eval(&xm)) / (2.0 * h);
            j.set_column(c, &col);
        }
        j
    };
    let mut f = eval(&x);
    let mut lambda = 1e-3;
    let mut converged = f.norm() < 1e-10;
    for _ in 0..200 {
        if converged || x.is_empty() {
            break;
        }
        let j = jac(&x, &f);
        let jt = j.transpose();
        let mut a = &jt * &j;
        let g = &jt * &f;
        let diag = a.diagonal();
        for k in 0..a.nrows() {
            a[(k, k)] += lambda * (1.0 + diag[k]);
        }
        let Some(step) = a.lu().solve(&(-g)) else { break };
        let xn = &x + &step;
        let fnew = eval(&xn);
        if fnew.norm() < f.norm() {
            x = xn;
            f = fnew;
            lambda = (lambda * 0.3).max(1e-12);
            converged = f.norm() < 1e-10 || step.norm() < 1e-14;
        } else {
            lambda *= 10.0;
            if lambda > 1e12 {
                break;
            }
        }
    }
    // Grados libres: incógnitas menos el rango del jacobiano en la solución
    let dof = if x.is_empty() {
        0
    } else {
        let j = jac(&x, &f);
        let rank = if j.nrows() == 0 { 0 } else { j.svd(false, false).rank(1e-6) };
        x.len() - rank
    };
    let poses = asm
        .instances
        .iter()
        .map(|i| {
            let (p, r) = pose(&x, i.id).unwrap();
            (i.id, p, r)
        })
        .collect();
    AssemblySolution { poses, residual: f.norm(), converged: f.norm() < 1e-6, dof }
}

/// Cada instancia como forma en su lugar (las piezas que ya no existen se saltean).
pub fn instance_shapes(ev: &Evaluation, asm: &Assembly, sol: &AssemblySolution) -> Vec<(u32, Shape)> {
    sol.poses
        .iter()
        .filter_map(|(id, p, r)| {
            let inst = asm.instances.iter().find(|i| i.id == *id)?;
            let part = ev.parts.iter().find(|x| x.id == inst.part)?;
            part.shape.transform(instance_matrix(*p, *r)).ok().map(|s| (*id, s))
        })
        .collect()
}

/// Volumen en común entre cada par de instancias que chocan (mm³).
pub fn interferences(shapes: &[(u32, Shape)]) -> Vec<(u32, u32, f64)> {
    let mut out = Vec::new();
    let boxes: Vec<_> = shapes.iter().map(|(_, s)| s.mass().ok().map(|m| (m.bbox_min, m.bbox_max))).collect();
    for i in 0..shapes.len() {
        for j in i + 1..shapes.len() {
            let (Some((a0, a1)), Some((b0, b1))) = (boxes[i], boxes[j]) else { continue };
            if (0..3).any(|k| a0[k] > b1[k] || b0[k] > a1[k]) {
                continue;
            }
            if let Ok(common) = shapes[i].1.intersect(&shapes[j].1)
                && let Ok(m) = common.mass()
                && m.volume > 1e-9
            {
                out.push((shapes[i].0, shapes[j].0, m.volume));
            }
        }
    }
    out
}
