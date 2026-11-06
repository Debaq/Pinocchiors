//! Soportes para resina (SLA4 de `PLAN_SLA.md`): puntos automáticos y su geometría.
//!
//! **Puntos**: en las caras que miran hacia abajo más planas que `overhang_angle` (se
//! reparten a una distancia `spacing` entre sí, de abajo hacia arriba), y uno en cada
//! isla que haya quedado sin punto. Los puntos se pueden editar a mano y volver a armar.
//!
//! **Geometría** de cada soporte, de arriba hacia abajo: punta fina que se clava un poco
//! en la pieza, columna vertical y un pie ancho sobre la balsa (o, si abajo hay pieza,
//! otra punta apoyada en ella). La balsa es la envolvente convexa de los pies.
//! Todo sale como triángulos que se suman a la pieza al laminar (las cáscaras se suman).

use serde::{Deserialize, Serialize};

use crate::sla_check::Island;

pub type Tri = [[f32; 3]; 3];

/// Cuánto soporte poner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportDensity {
    Light,
    Medium,
    Heavy,
}

/// Medidas de los soportes (mm y grados).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SupportSettings {
    /// Distancia entre puntos en las caras en voladizo.
    pub spacing: f32,
    /// Las caras que miran hacia abajo con menos de este ángulo sobre la horizontal llevan soporte.
    pub overhang_angle: f32,
    pub tip_diameter: f32,
    pub tip_length: f32,
    /// Cuánto se clava la punta en la pieza.
    pub tip_penetration: f32,
    pub pillar_diameter: f32,
    pub foot_diameter: f32,
    pub foot_height: f32,
    /// Altura de la pieza sobre la balsa.
    pub lift: f32,
    pub raft_thickness: f32,
    /// Cuánto sobresale la balsa de los pies.
    pub raft_margin: f32,
    /// Permitir soportes que se apoyan en la propia pieza.
    pub on_model: bool,
    /// Lados de cada columna.
    pub segments: u32,
    /// Punta en la dirección de la normal de la cara (inclinada hasta `tip_max_angle` °
    /// respecto de la vertical) en lugar de vertical.
    pub tip_normal: bool,
    pub tip_max_angle: f32,
    /// Refuerzos en zigzag entre columnas vecinas a menos de `brace_max_distance` mm.
    pub braces: bool,
    pub brace_diameter: f32,
    pub brace_max_distance: f32,
    /// Ramas: columnas cercanas (a menos de `tree_radius` mm, hasta `tree_branches` por
    /// tronco) se juntan a 45° en un tronco: menos resina y menos marcas en la balsa.
    pub tree: bool,
    pub tree_radius: f32,
    pub tree_branches: u32,
    /// Chaflán del borde de arriba de la balsa (mm).
    pub raft_bevel: f32,
}

impl Default for SupportSettings {
    fn default() -> Self {
        Self::preset(SupportDensity::Medium)
    }
}

impl SupportSettings {
    pub fn preset(d: SupportDensity) -> Self {
        let (spacing, tip, pillar) = match d {
            SupportDensity::Light => (3.5, 0.3, 0.8),
            SupportDensity::Medium => (2.5, 0.4, 1.0),
            SupportDensity::Heavy => (1.8, 0.5, 1.4),
        };
        Self {
            spacing,
            overhang_angle: 45.0,
            tip_diameter: tip,
            tip_length: 1.5,
            tip_penetration: 0.2,
            pillar_diameter: pillar,
            foot_diameter: pillar * 2.2,
            foot_height: 0.6,
            lift: 5.0,
            raft_thickness: 1.0,
            raft_margin: 1.5,
            on_model: true,
            segments: 8,
            tip_normal: true,
            tip_max_angle: 45.0,
            braces: true,
            brace_diameter: pillar * 0.6,
            brace_max_distance: spacing * 3.0,
            tree: false,
            tree_radius: spacing * 2.0,
            tree_branches: 4,
            raft_bevel: 0.6,
        }
    }

    /// Altura de la cara de abajo de la pieza una vez elevada.
    pub fn model_bottom(&self) -> f32 {
        self.raft_thickness + self.lift
    }
}

/// De dónde salió un punto de soporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointKind {
    Overhang,
    Island,
    Manual,
}

/// Un punto donde la punta toca la pieza (mm).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SupportPoint {
    pub pos: [f32; 3],
    pub kind: PointKind,
    /// Diámetro de la punta de este punto (mm); sin valor, el de los ajustes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip: Option<f32>,
}

/// Puntos automáticos: caras en voladizo e islas. `adjust` corre o descarta un punto
/// (venteos): `None` lo descarta; a los de voladizo solo se los deja o descarta.
pub fn auto_points(
    model: &[Tri],
    islands: &[Island],
    s: &SupportSettings,
    adjust: impl Fn(&[f32; 3]) -> Option<[f32; 3]>,
) -> Vec<SupportPoint> {
    let cos = s.overhang_angle.to_radians().cos();
    let step = (s.spacing * 0.5).max(0.05);
    let mut cand: Vec<[f32; 3]> = Vec::new();
    for t in model {
        let n = unit_normal(t);
        if -n[2] < cos {
            continue;
        }
        let longest = (0..3).map(|e| dist(&t[e], &t[(e + 1) % 3])).fold(0f32, f32::max);
        let k = (longest / step).ceil().max(1.0) as usize;
        // Grilla por dentro del triángulo (en las aristas el rayo hacia abajo es ambiguo).
        for i in 0..k {
            for j in 0..k - i {
                let (u, v) = ((i as f32 + 1.0 / 3.0) / k as f32, (j as f32 + 1.0 / 3.0) / k as f32);
                cand.push(std::array::from_fn(|c| t[0][c] + u * (t[1][c] - t[0][c]) + v * (t[2][c] - t[0][c])));
            }
        }
    }
    // Lo más bajo primero: es lo que primero se forma en el aire.
    cand.sort_by(|a, b| a[2].total_cmp(&b[2]));

    let mut grid = PointGrid::new(s.spacing);
    let mut points = Vec::new();
    for p in cand {
        if adjust(&p) == Some(p) && !grid.near(&p, s.spacing, s.spacing) {
            grid.insert(p);
            points.push(SupportPoint { pos: p, kind: PointKind::Overhang, tip: None });
        }
    }
    // Toda isla lleva su punto salvo que ya haya uno casi encima (las chicas no se tocan
    // con un punto a más de una punta de distancia).
    let reach = (s.tip_diameter * 1.5).max(0.5);
    for i in islands {
        let Some(p) = adjust(&i.pos) else { continue };
        if !grid.near(&p, reach, 0.5) {
            grid.insert(p);
            points.push(SupportPoint { pos: p, kind: PointKind::Island, tip: None });
        }
    }
    points
}

/// Resultado de armar los soportes.
#[derive(Debug, Clone, Default)]
pub struct Supports {
    pub triangles: Vec<Tri>,
    /// Soportes hasta la balsa y apoyados en la pieza.
    pub to_raft: usize,
    pub on_model: usize,
    /// Columnas hasta la balsa (con ramas, menos que `to_raft`), puntas inclinadas y refuerzos.
    pub pillars: usize,
    pub tilted: usize,
    pub braces: usize,
    /// Puntos que no se pudieron poner y por qué.
    pub skipped: Vec<(usize, SkipReason)>,
}

/// Por qué no se puso un soporte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Lo de abajo es la misma pieza a menos de 0,5 mm.
    TooClose,
    /// El punto quedó dentro de la pieza.
    Inside,
    /// Abajo hay pieza y no se permiten soportes sobre la pieza.
    OnModelOff,
    /// Abajo hay un agujero de venteo.
    Vent,
    /// El punto está casi sobre la balsa.
    TooLow,
}

/// Cómo baja un punto que va hasta la balsa: la punta de `inner` (dentro de la pieza) a
/// `q`, y desde `q` la columna.
#[derive(Debug, Clone, Copy)]
struct ToRaft {
    inner: [f32; 3],
    q: [f32; 3],
    tip_r: f32,
}

/// Una columna vertical hasta la balsa (para los refuerzos).
#[derive(Debug, Clone, Copy)]
struct Pillar {
    xy: [f32; 2],
    top: f32,
    r: f32,
}

/// Arma los soportes de `points` bajo la pieza `model` (ya elevada). `blocked(x, y, z)`
/// dice dónde no se puede apoyar un pie sobre la pieza ni pasar una rama (venteos).
pub fn build(model: &[Tri], points: &[SupportPoint], s: &SupportSettings, blocked: impl Fn(f32, f32, f32) -> bool) -> Supports {
    let seg = s.segments.max(3) as usize;
    let (pillar_r, foot_r) = (s.pillar_diameter / 2.0, s.foot_diameter / 2.0);
    let raft_top = s.raft_thickness;
    let base_top = raft_top + s.foot_height;
    let grid = ZGrid::new(model, s.spacing.max(1.0));
    let clear = |a: [f32; 3], b: [f32; 3], r: f32| segment_clear(&grid, &blocked, a, b, r);
    let mut out = Supports::default();
    let mut to_raft: Vec<ToRaft> = Vec::new();

    for (idx, sp) in points.iter().enumerate() {
        let [x, y, z] = sp.pos;
        let tip_r = sp.tip.map_or(s.tip_diameter, |d| d.max(0.05)) / 2.0;
        let top = z + s.tip_penetration;
        // Punta según la normal de la cara: solo si desde ahí la columna baja libre a la balsa.
        if s.tip_normal
            && let Some(n) = grid.nearest_normal(&sp.pos)
            && let Some(t) = tip_dir(n, s.tip_max_angle)
        {
            let q = add(sp.pos, t, s.tip_length);
            let inner = add(sp.pos, t, -s.tip_penetration);
            let free_below = grid.below(q[0], q[1], q[2]).is_none_or(|(h, _)| h <= raft_top);
            if q[2] > base_top + 0.3 && free_below && clear(add(sp.pos, t, s.tip_length * 0.8), q, pillar_r) {
                to_raft.push(ToRaft { inner, q, tip_r });
                out.tilted += 1;
                continue;
            }
        }
        // Lo primero que hay abajo (sin contar la cara del punto).
        match grid.below(x, y, z - 0.05) {
            // Lo primero de abajo mira hacia abajo: el punto quedó dentro de la pieza.
            Some((_, false)) => {
                out.skipped.push((idx, SkipReason::Inside));
                continue;
            }
            Some((hit, true)) if hit > raft_top => {
                let gap = z - hit;
                let why = if !s.on_model {
                    Some(SkipReason::OnModelOff)
                } else if gap < 0.5 {
                    Some(SkipReason::TooClose)
                } else if blocked(x, y, hit) {
                    Some(SkipReason::Vent)
                } else {
                    None
                };
                if let Some(why) = why {
                    out.skipped.push((idx, why));
                    continue;
                }
                let bottom = hit - s.tip_penetration;
                if gap >= 2.0 * s.tip_length + 0.2 {
                    let (a, b) = (z - s.tip_length, hit + s.tip_length);
                    frustum(&mut out.triangles, [x, y, a], pillar_r, [x, y, top], tip_r, seg);
                    frustum(&mut out.triangles, [x, y, b - 0.05], pillar_r, [x, y, a + 0.05], pillar_r, seg);
                    frustum(&mut out.triangles, [x, y, bottom], tip_r, [x, y, b], pillar_r, seg);
                } else {
                    let mid = (z + hit) / 2.0;
                    let r = pillar_r.min(gap / 4.0).max(tip_r);
                    frustum(&mut out.triangles, [x, y, mid], r, [x, y, top], tip_r, seg);
                    frustum(&mut out.triangles, [x, y, bottom], tip_r, [x, y, mid + 0.02], r, seg);
                }
                out.on_model += 1;
            }
            _ => {
                if z <= base_top + 0.3 {
                    out.skipped.push((idx, SkipReason::TooLow));
                    continue;
                }
                let a = (z - s.tip_length).max(base_top + 0.1);
                to_raft.push(ToRaft { inner: [x, y, top], q: [x, y, a], tip_r });
            }
        }
    }

    // Puntas, ramas y columnas de lo que baja a la balsa.
    let mut pillars: Vec<Pillar> = Vec::new();
    let groups = if s.tree { tree_groups(&to_raft, s, base_top, &clear) } else { to_raft.iter().map(|t| (vec![t.q], t.q)).collect() };
    for t in &to_raft {
        tube(&mut out.triangles, t.q, pillar_r, t.inner, t.tip_r, seg);
    }
    for (members, j) in groups {
        let r = if members.len() > 1 { (pillar_r * (1.0 + 0.25 * (members.len() - 1) as f32)).min(pillar_r * 2.0) } else { pillar_r };
        if members.len() > 1 {
            for q in &members {
                tube(&mut out.triangles, *q, pillar_r, j, pillar_r, seg);
            }
        }
        frustum(&mut out.triangles, [j[0], j[1], base_top - 0.05], r, [j[0], j[1], j[2] + 0.05], r, seg);
        frustum(&mut out.triangles, [j[0], j[1], raft_top - 0.05], foot_r.max(r * 2.2), [j[0], j[1], base_top], r, seg);
        pillars.push(Pillar { xy: [j[0], j[1]], top: j[2], r });
    }
    out.to_raft = to_raft.len();
    out.pillars = pillars.len();

    if s.braces {
        out.braces = braces(&mut out.triangles, &pillars, s, base_top, seg, &clear);
    }

    if !pillars.is_empty() && s.raft_thickness > 0.0 {
        let feet: Vec<[f32; 2]> = pillars.iter().map(|p| p.xy).collect();
        raft(&mut out.triangles, &feet, foot_r + s.raft_margin, s.raft_thickness, s.raft_bevel);
    }
    out
}

/// Dirección de la punta desde el punto: la normal de la cara (hacia afuera), inclinada
/// como mucho `max_deg` respecto de la vertical. `None` si la cara mira hacia arriba o
/// la punta saldría vertical (eso lo arma el camino de siempre).
fn tip_dir(n: [f32; 3], max_deg: f32) -> Option<[f32; 3]> {
    let h = (n[0] * n[0] + n[1] * n[1]).sqrt();
    if n[2] > 0.0 || h < 0.05 {
        return None;
    }
    let tilt = h.atan2(-n[2]).min(max_deg.clamp(0.0, 80.0).to_radians());
    if tilt < 3f32.to_radians() {
        return None;
    }
    Some([n[0] / h * tilt.sin(), n[1] / h * tilt.sin(), -tilt.cos()])
}

/// Agrupa en árboles las columnas cercanas: ramas a 45° desde cada `q` hasta un tronco bajo
/// el centro del grupo. Devuelve por grupo los extremos de las ramas y la unión.
fn tree_groups(
    to_raft: &[ToRaft],
    s: &SupportSettings,
    base_top: f32,
    clear: &impl Fn([f32; 3], [f32; 3], f32) -> bool,
) -> Vec<(Vec<[f32; 3]>, [f32; 3])> {
    let r = s.pillar_diameter / 2.0;
    let mut order: Vec<usize> = (0..to_raft.len()).collect();
    // Los más altos primero: son los que más columna ahorran.
    order.sort_by(|&a, &b| to_raft[b].q[2].total_cmp(&to_raft[a].q[2]));
    let mut used = vec![false; to_raft.len()];
    let mut out = Vec::new();
    for &i in &order {
        if used[i] {
            continue;
        }
        used[i] = true;
        let qi = to_raft[i].q;
        let mut near: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&j| !used[j] && dist2(qi, to_raft[j].q) <= s.tree_radius * s.tree_radius)
            .collect();
        near.sort_by(|&a, &b| dist2(qi, to_raft[a].q).total_cmp(&dist2(qi, to_raft[b].q)));
        near.truncate(s.tree_branches.max(1) as usize - 1);
        let mut members = vec![i];
        members.extend(near);
        loop {
            if members.len() == 1 {
                out.push((vec![qi], qi));
                break;
            }
            let qs: Vec<[f32; 3]> = members.iter().map(|&m| to_raft[m].q).collect();
            let n = qs.len() as f32;
            let c = [qs.iter().map(|q| q[0]).sum::<f32>() / n, qs.iter().map(|q| q[1]).sum::<f32>() / n];
            let jz = qs.iter().map(|q| q[2] - (dist2(*q, [c[0], c[1], q[2]])).sqrt()).fold(f32::MAX, f32::min) - 0.5;
            let j = [c[0], c[1], jz];
            let trunk_r = (r * (1.0 + 0.25 * (n - 1.0))).min(r * 2.0);
            let ok_branches: Vec<bool> = qs.iter().map(|q| clear(*q, j, r)).collect();
            if jz >= base_top + 2.0 && ok_branches.iter().all(|&b| b) && clear(j, [c[0], c[1], base_top + 0.5], trunk_r) {
                for &m in &members[1..] {
                    used[m] = true;
                }
                out.push((qs, j));
                break;
            }
            // Se saca la rama que falla (o la más lejana) y se prueba de nuevo.
            let drop = ok_branches
                .iter()
                .skip(1)
                .position(|&b| !b)
                .map(|k| k + 1)
                .unwrap_or(members.len() - 1);
            members.remove(drop);
        }
    }
    out
}

/// Refuerzos en zigzag entre cada columna y su vecina más cercana (si están a menos de
/// `brace_max_distance` y la columna da para al menos un tramo a 45°).
fn braces(out: &mut Vec<Tri>, pillars: &[Pillar], s: &SupportSettings, base_top: f32, seg: usize, clear: &impl Fn([f32; 3], [f32; 3], f32) -> bool) -> usize {
    let r = s.brace_diameter / 2.0;
    let mut count = 0;
    let mut pairs = std::collections::BTreeSet::new();
    for (i, a) in pillars.iter().enumerate() {
        let best = pillars
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(j, b)| (j, (a.xy[0] - b.xy[0]).hypot(a.xy[1] - b.xy[1])))
            .filter(|&(_, d)| d > a.r + 0.2 && d <= s.brace_max_distance)
            .min_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((j, _)) = best {
            pairs.insert((i.min(j), i.max(j)));
        }
    }
    for (i, j) in pairs {
        let (a, b) = (&pillars[i], &pillars[j]);
        let d = (a.xy[0] - b.xy[0]).hypot(a.xy[1] - b.xy[1]);
        let (lo, hi) = (base_top + 0.3, a.top.min(b.top) - 0.3);
        let mut z = lo;
        let mut from_a = true;
        while z + d <= hi {
            let (p, q) = if from_a { (a.xy, b.xy) } else { (b.xy, a.xy) };
            let (p, q) = ([p[0], p[1], z], [q[0], q[1], z + d]);
            if clear(p, q, r) {
                tube(out, p, r, q, r, seg);
                count += 1;
            }
            z += d;
            from_a = !from_a;
        }
    }
    count
}

/// Balsa: envolvente de los pies (radio `r`), con la cara de arriba achicada `bevel` mm
/// para que el borde quede en chaflán (se despega más fácil de la placa con una espátula).
fn raft(out: &mut Vec<Tri>, feet: &[[f32; 2]], r: f32, thickness: f32, bevel: f32) {
    const N: usize = 16;
    let ring = |f: &[f32; 2], k: usize, r: f32| {
        let a = k as f32 / N as f32 * std::f32::consts::TAU;
        [f[0] + r * a.cos(), f[1] + r * a.sin()]
    };
    let pts: Vec<[f32; 2]> = feet.iter().flat_map(|f| (0..N).map(move |k| ring(f, k, r))).collect();
    // La envolvente de los mismos círculos más chicos sale de los mismos (pie, ángulo).
    let hull = convex_hull_indices(&pts);
    let bevel = bevel.clamp(0.0, (thickness * 0.8).min(r * 0.8));
    let top_z = thickness;
    let low: Vec<[f32; 2]> = hull.iter().map(|&i| pts[i]).collect();
    if bevel <= 0.0 || low.len() < 3 {
        prism(out, &low, 0.0, top_z);
        return;
    }
    let high: Vec<[f32; 2]> = hull.iter().map(|&i| ring(&feet[i / N], i % N, r - bevel)).collect();
    let mid_z = top_z - bevel;
    let n = low.len();
    let p = |poly: &[[f32; 2]], i: usize, z: f32| [poly[i % n][0], poly[i % n][1], z];
    for i in 1..n - 1 {
        out.push([p(&low, 0, 0.0), p(&low, i + 1, 0.0), p(&low, i, 0.0)]);
        out.push([p(&high, 0, top_z), p(&high, i, top_z), p(&high, i + 1, top_z)]);
    }
    for i in 0..n {
        // Pared vertical hasta `mid_z` y chaflán hasta arriba.
        out.push([p(&low, i, 0.0), p(&low, i + 1, 0.0), p(&low, i + 1, mid_z)]);
        out.push([p(&low, i, 0.0), p(&low, i + 1, mid_z), p(&low, i, mid_z)]);
        out.push([p(&low, i, mid_z), p(&low, i + 1, mid_z), p(&high, i + 1, top_z)]);
        out.push([p(&low, i, mid_z), p(&high, i + 1, top_z), p(&high, i, top_z)]);
    }
}

/// ¿El tramo `a`–`b` de radio `r` pasa sin tocar la pieza ni un venteo? Rayos verticales
/// cada 0,25 mm por el eje y a los costados.
fn segment_clear(grid: &ZGrid, blocked: &impl Fn(f32, f32, f32) -> bool, a: [f32; 3], b: [f32; 3], r: f32) -> bool {
    let len = dist2(a, b).sqrt();
    let n = (len / 0.25).ceil().max(1.0) as usize;
    let c = r + 0.2;
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let p: [f32; 3] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        for (dx, dy) in [(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
            let (x, y) = (p[0] + dx, p[1] + dy);
            if blocked(x, y, p[2]) {
                return false;
            }
            // Pieza a menos de `c` arriba o abajo, o el punto dentro de la pieza.
            if let Some((h, up)) = grid.below(x, y, p[2] + c)
                && (h > p[2] - c || !up)
            {
                return false;
            }
        }
    }
    true
}

fn add(p: [f32; 3], d: [f32; 3], k: f32) -> [f32; 3] {
    [p[0] + d[0] * k, p[1] + d[1] * k, p[2] + d[2] * k]
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Tubo cerrado de `a` (radio `ra`) a `b` (radio `rb`) con las tapas perpendiculares al eje.
pub fn tube(out: &mut Vec<Tri>, a: [f32; 3], ra: f32, b: [f32; 3], rb: f32, seg: usize) {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if l < 1e-6 {
        return;
    }
    let d = d.map(|c| c / l);
    let h = if d[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
    let u = normalize(cross(h, d));
    let v = cross(d, u);
    let ring = |c: [f32; 3], r: f32, i: usize| {
        let t = (i % seg) as f32 / seg as f32 * std::f32::consts::TAU;
        let (cs, sn) = (t.cos() * r, t.sin() * r);
        [c[0] + u[0] * cs + v[0] * sn, c[1] + u[1] * cs + v[1] * sn, c[2] + u[2] * cs + v[2] * sn]
    };
    for i in 0..seg {
        let (b0, b1, t0, t1) = (ring(a, ra, i), ring(a, ra, i + 1), ring(b, rb, i), ring(b, rb, i + 1));
        out.push([b0, b1, t1]);
        out.push([b0, t1, t0]);
        out.push([a, b1, b0]);
        out.push([b, t0, t1]);
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > 0.0 { a.map(|c| c / l) } else { a }
}

/// Tronco de cono cerrado entre `c0` (abajo, radio `r0`) y `c1` (arriba, radio `r1`), normales afuera.
pub fn frustum(out: &mut Vec<Tri>, c0: [f32; 3], r0: f32, c1: [f32; 3], r1: f32, seg: usize) {
    let ring = |c: [f32; 3], r: f32, i: usize| {
        let a = (i % seg) as f32 / seg as f32 * std::f32::consts::TAU;
        [c[0] + r * a.cos(), c[1] + r * a.sin(), c[2]]
    };
    for i in 0..seg {
        let (b0, b1, t0, t1) = (ring(c0, r0, i), ring(c0, r0, i + 1), ring(c1, r1, i), ring(c1, r1, i + 1));
        out.push([b0, b1, t1]);
        out.push([b0, t1, t0]);
        out.push([c0, b1, b0]);
        out.push([c1, t0, t1]);
    }
}

/// Prisma vertical sobre un polígono convexo en sentido antihorario.
fn prism(out: &mut Vec<Tri>, poly: &[[f32; 2]], z0: f32, z1: f32) {
    let n = poly.len();
    if n < 3 {
        return;
    }
    let p = |i: usize, z: f32| [poly[i % n][0], poly[i % n][1], z];
    for i in 1..n - 1 {
        out.push([p(0, z0), p(i + 1, z0), p(i, z0)]);
        out.push([p(0, z1), p(i, z1), p(i + 1, z1)]);
    }
    for i in 0..n {
        out.push([p(i, z0), p(i + 1, z0), p(i + 1, z1)]);
        out.push([p(i, z0), p(i + 1, z1), p(i, z1)]);
    }
}

/// Envolvente convexa (cadena monótona), en sentido antihorario.
#[cfg(test)]
fn convex_hull(pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    convex_hull_indices(&pts).into_iter().map(|i| pts[i]).collect()
}

/// Índices de la envolvente convexa de `pts`, en sentido antihorario.
fn convex_hull_indices(pts: &[[f32; 2]]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    idx.sort_by(|&a, &b| pts[a][0].total_cmp(&pts[b][0]).then(pts[a][1].total_cmp(&pts[b][1])));
    idx.dedup_by(|a, b| pts[*a] == pts[*b]);
    if idx.len() < 3 {
        return idx;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut hull: Vec<usize> = Vec::with_capacity(idx.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &usize>> = if pass == 0 { Box::new(idx.iter()) } else { Box::new(idx.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(pts[hull[hull.len() - 2]], pts[hull[hull.len() - 1]], pts[p]) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

fn unit_normal(t: &Tri) -> [f32; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 0.0 { n.map(|c| c / l) } else { [0.0; 3] }
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Puntos en una grilla XY para preguntar «¿hay uno cerca?».
struct PointGrid {
    cell: f32,
    map: std::collections::HashMap<(i32, i32), Vec<[f32; 3]>>,
}

impl PointGrid {
    fn new(cell: f32) -> Self {
        Self { cell: cell.max(0.1), map: Default::default() }
    }
    fn key(&self, x: f32, y: f32) -> (i32, i32) {
        ((x / self.cell).floor() as i32, (y / self.cell).floor() as i32)
    }
    fn insert(&mut self, p: [f32; 3]) {
        let k = self.key(p[0], p[1]);
        self.map.entry(k).or_default().push(p);
    }
    /// Hay un punto a menos de `r` en XY y de `dz` en altura.
    fn near(&self, p: &[f32; 3], r: f32, dz: f32) -> bool {
        let (kx, ky) = self.key(p[0], p[1]);
        let reach = (r / self.cell).ceil() as i32;
        for i in -reach..=reach {
            for j in -reach..=reach {
                if let Some(v) = self.map.get(&(kx + i, ky + j))
                    && v.iter().any(|q| (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) < r * r && (q[2] - p[2]).abs() < dz)
                {
                    return true;
                }
            }
        }
        false
    }
}

/// Triángulos repartidos en una grilla XY para tirar rayos verticales.
pub(crate) struct ZGrid<'a> {
    tris: &'a [Tri],
    cell: f32,
    min: [f32; 2],
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

impl<'a> ZGrid<'a> {
    pub(crate) fn new(tris: &'a [Tri], cell: f32) -> Self {
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for v in tris.iter().flatten() {
            for c in 0..2 {
                min[c] = min[c].min(v[c]);
                max[c] = max[c].max(v[c]);
            }
        }
        if tris.is_empty() {
            return Self { tris, cell, min: [0.0; 2], nx: 0, ny: 0, cells: Vec::new() };
        }
        let nx = (((max[0] - min[0]) / cell).floor() as usize + 1).min(4096);
        let ny = (((max[1] - min[1]) / cell).floor() as usize + 1).min(4096);
        let cell = cell.max((max[0] - min[0]) / nx as f32).max((max[1] - min[1]) / ny as f32);
        let mut cells = vec![Vec::new(); nx * ny];
        for (i, t) in tris.iter().enumerate() {
            let lo = |c: usize| t.iter().map(|v| v[c]).fold(f32::MAX, f32::min);
            let hi = |c: usize| t.iter().map(|v| v[c]).fold(f32::MIN, f32::max);
            let ix = |x: f32| (((x - min[0]) / cell) as usize).min(nx - 1);
            let iy = |y: f32| (((y - min[1]) / cell) as usize).min(ny - 1);
            for gy in iy(lo(1))..=iy(hi(1)) {
                for gx in ix(lo(0))..=ix(hi(0)) {
                    cells[gy * nx + gx].push(i as u32);
                }
            }
        }
        Self { tris, cell, min, nx, ny, cells }
    }

    /// Normal de la cara más cercana a `p` (buscando en su celda y las vecinas).
    pub(crate) fn nearest_normal(&self, p: &[f32; 3]) -> Option<[f32; 3]> {
        if self.nx == 0 {
            return None;
        }
        let gx = ((p[0] - self.min[0]) / self.cell).floor() as i64;
        let gy = ((p[1] - self.min[1]) / self.cell).floor() as i64;
        let mut best: Option<(f32, usize)> = None;
        for cy in gy - 1..=gy + 1 {
            for cx in gx - 1..=gx + 1 {
                if cx < 0 || cy < 0 || cx as usize >= self.nx || cy as usize >= self.ny {
                    continue;
                }
                for &i in &self.cells[cy as usize * self.nx + cx as usize] {
                    let d = point_triangle_dist2(p, &self.tris[i as usize]);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, i as usize));
                    }
                }
            }
        }
        best.map(|(_, i)| unit_normal(&self.tris[i]))
    }

    /// La z más alta de la malla en la vertical `(x, y)` que esté por debajo de `z`, y si
    /// esa cara mira hacia arriba.
    pub(crate) fn below(&self, x: f32, y: f32, z: f32) -> Option<(f32, bool)> {
        let (fx, fy) = ((x - self.min[0]) / self.cell, (y - self.min[1]) / self.cell);
        if self.nx == 0 || fx < 0.0 || fy < 0.0 || fx as usize >= self.nx || fy as usize >= self.ny {
            return None;
        }
        let mut best: Option<(f32, bool)> = None;
        for &i in &self.cells[fy as usize * self.nx + fx as usize] {
            let t = &self.tris[i as usize];
            if let Some(h) = vertical_hit(t, x, y)
                && h < z
                && best.is_none_or(|b| h > b.0)
            {
                best = Some((h, unit_normal(t)[2] > 0.0));
            }
        }
        best
    }
}

/// Distancia al cuadrado de `p` al triángulo `t` (punto más cercano, como en Ericson).
fn point_triangle_dist2(p: &[f32; 3], t: &Tri) -> f32 {
    let sub = |a: &[f32; 3], b: &[f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (a, b, c) = (&t[0], &t[1], &t[2]);
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let at = |u: f32, v: f32| -> f32 {
        let q = [a[0] + ab[0] * u + ac[0] * v, a[1] + ab[1] * u + ac[1] * v, a[2] + ab[2] * u + ac[2] * v];
        dot(sub(p, &q), sub(p, &q))
    };
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return at(0.0, 0.0);
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return at(1.0, 0.0);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return at(d1 / (d1 - d3), 0.0);
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return at(0.0, 1.0);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return at(0.0, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return at(1.0 - w, w);
    }
    let den = 1.0 / (va + vb + vc);
    at(vb * den, vc * den)
}

/// Altura del triángulo en `(x, y)` si la vertical lo atraviesa.
fn vertical_hit(t: &Tri, x: f32, y: f32) -> Option<f32> {
    let (a, b, c) = (t[0], t[1], t[2]);
    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if d.abs() < 1e-12 {
        return None;
    }
    let l0 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
    let l1 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
    let l2 = 1.0 - l0 - l1;
    let e = -1e-6;
    (l0 >= e && l1 >= e && l2 >= e).then(|| l0 * a[2] + l1 * b[2] + l2 * c[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sla::tests::cube;

    #[test]
    fn cara_de_abajo_lleva_grilla_de_puntos() {
        let s = SupportSettings::default();
        let model = cube([10.0, 10.0, 6.0], [10.0, 10.0, 5.0]);
        let pts = auto_points(&model, &[], &s, |p| Some(*p));
        // 10 × 10 mm a 2,5 mm: entre 12 y 36 puntos, todos en la cara de abajo, y ningún
        // lugar de la cara a más de 1,5 × la distancia de un punto.
        assert!((12..=36).contains(&pts.len()), "{}", pts.len());
        for i in 0..=20 {
            for j in 0..=20 {
                let (x, y) = (10.0 + i as f32 * 0.5, 10.0 + j as f32 * 0.5);
                let d = pts.iter().map(|p| ((p.pos[0] - x).powi(2) + (p.pos[1] - y).powi(2)).sqrt()).fold(f32::MAX, f32::min);
                assert!(d <= s.spacing * 1.5, "({x}, {y}) a {d} mm");
            }
        }
        assert!(pts.iter().all(|p| (p.pos[2] - 6.0).abs() < 1e-4));
        for (i, a) in pts.iter().enumerate() {
            for b in &pts[i + 1..] {
                assert!(((a.pos[0] - b.pos[0]).powi(2) + (a.pos[1] - b.pos[1]).powi(2)).sqrt() >= s.spacing - 1e-4);
            }
        }
    }

    #[test]
    fn paredes_verticales_no_llevan_puntos() {
        let s = SupportSettings::default();
        let model = cube([10.0, 10.0, 0.0], [10.0, 10.0, 5.0]);
        let pts = auto_points(&model, &[], &s, |p| (p[2] > 0.5).then_some(*p));
        assert!(pts.is_empty());
    }

    #[test]
    fn soporte_sobre_la_pieza_y_hasta_la_balsa() {
        let s = SupportSettings::default();
        // Mesa: tablero arriba (z 10–11) y un bloque abajo (z 6–8) bajo la mitad izquierda.
        let mut model = cube([10.0, 10.0, 10.0], [10.0, 4.0, 1.0]);
        model.extend(cube([10.0, 10.0, 6.0], [4.0, 4.0, 2.0]));
        let points = [
            SupportPoint { pos: [12.0, 12.0, 10.0], kind: PointKind::Manual, tip: None },
            SupportPoint { pos: [18.0, 12.0, 10.0], kind: PointKind::Manual, tip: None },
            SupportPoint { pos: [12.0, 12.0, 10.3], kind: PointKind::Manual, tip: None },
        ];
        let sup = build(&model, &points, &s, |_, _, _| false);
        assert_eq!(sup.on_model, 1);
        assert_eq!(sup.to_raft, 1);
        assert_eq!(sup.skipped, vec![(2, SkipReason::Inside)]);
        let zmin = sup.triangles.iter().flatten().map(|v| v[2]).fold(f32::MAX, f32::min);
        assert!(zmin.abs() < 1e-6);
    }

    /// Placa de `w` × `w` mm a la altura `z`, girada `deg` ° alrededor de Y.
    fn tilted_plate(z: f32, w: f32, deg: f32) -> Vec<Tri> {
        let (c, sn) = (deg.to_radians().cos(), deg.to_radians().sin());
        cube([-w / 2.0, -w / 2.0, -0.5], [w, w, 1.0])
            .into_iter()
            .map(|t| t.map(|v| [20.0 + v[0] * c - v[2] * sn, 20.0 + v[1], z + v[0] * sn + v[2] * c]))
            .collect()
    }

    fn points_on(model: &[Tri], s: &SupportSettings) -> Vec<SupportPoint> {
        auto_points(model, &[], s, |p| Some(*p))
    }

    #[test]
    fn punta_segun_la_normal() {
        assert!(tip_dir([0.0, 0.0, -1.0], 45.0).is_none());
        let t = tip_dir([0.5, 0.0, -0.866], 45.0).unwrap();
        assert!((t[0] - 0.5).abs() < 1e-3 && (t[2] + 0.866).abs() < 1e-3);
        // Una pared casi vertical: se limita a 45°.
        let t = tip_dir([1.0, 0.0, -0.1], 45.0).unwrap();
        assert!((t[0] - t[2].abs()).abs() < 1e-3);
        assert!(tip_dir([0.0, 1.0, 0.2], 45.0).is_none());

        let s = SupportSettings { braces: false, ..Default::default() };
        let model = tilted_plate(15.0, 10.0, 30.0);
        let pts = points_on(&model, &s);
        assert!(!pts.is_empty());
        let sup = build(&model, &pts, &s, |_, _, _| false);
        assert_eq!(sup.tilted, pts.len());
        assert!(sup.skipped.is_empty());
        let vertical = build(&model, &pts, &SupportSettings { tip_normal: false, ..s }, |_, _, _| false);
        assert_eq!(vertical.tilted, 0);
        assert_eq!(vertical.to_raft, pts.len());
    }

    #[test]
    fn ramas_juntan_columnas() {
        let s = SupportSettings { tree: true, braces: false, ..Default::default() };
        let model = cube([10.0, 10.0, 25.0], [12.0, 12.0, 2.0]);
        let pts = points_on(&model, &s);
        let sup = build(&model, &pts, &s, |_, _, _| false);
        assert_eq!(sup.to_raft, pts.len());
        assert!(sup.pillars * 2 <= pts.len(), "{} columnas para {} puntos", sup.pillars, pts.len());
        // Sin ramas, una columna por punto.
        let plain = build(&model, &pts, &SupportSettings { tree: false, ..s }, |_, _, _| false);
        assert_eq!(plain.pillars, pts.len());
    }

    #[test]
    fn las_ramas_no_atraviesan_la_pieza() {
        // Tablero alto con un bloque debajo en el centro: las ramas no pueden pasar por él.
        let s = SupportSettings { tree: true, braces: true, on_model: false, ..Default::default() };
        let mut model = cube([0.0, 0.0, 20.0], [20.0, 20.0, 1.0]);
        // (Pegado al tablero; su cara de abajo lleva soportes que se clavan 0,2 mm.)
        let block = cube([7.0, 7.0, 6.0], [6.0, 6.0, 14.0]);
        model.extend(block.iter().copied());
        let pts = points_on(&model, &s);
        let sup = build(&model, &pts, &s, |_, _, _| false);
        let inside = |v: &[f32; 3]| (7.05..12.95).contains(&v[0]) && (7.05..12.95).contains(&v[1]) && (6.3..19.9).contains(&v[2]);
        let bad: Vec<_> = sup.triangles.iter().flatten().filter(|v| inside(v)).take(5).collect();
        assert!(bad.is_empty(), "un soporte entra en el bloque: {bad:?}");
    }

    #[test]
    fn refuerzos_en_columnas_altas() {
        let s = SupportSettings::default();
        let model = cube([10.0, 10.0, 30.0], [8.0, 8.0, 2.0]);
        let pts = points_on(&model, &s);
        let sup = build(&model, &pts, &s, |_, _, _| false);
        assert!(sup.braces > pts.len(), "{} refuerzos", sup.braces);
        let low = cube([10.0, 10.0, 4.5], [8.0, 8.0, 2.0]);
        let sup = build(&low, &points_on(&low, &s), &s, |_, _, _| false);
        assert_eq!(sup.braces, 0, "columnas cortas sin refuerzos");
    }

    #[test]
    fn balsa_con_chaflan() {
        let mut out = Vec::new();
        raft(&mut out, &[[0.0, 0.0], [10.0, 0.0]], 2.0, 1.0, 0.5);
        let ext = |z: f32| out.iter().flatten().filter(|v| (v[2] - z).abs() < 1e-4).map(|v| v[0]).fold(f32::MIN, f32::max);
        assert!((ext(0.0) - 12.0).abs() < 1e-3);
        assert!((ext(1.0) - 11.5).abs() < 1e-3);
        // Cerrada: cada arista aparece dos veces, en sentidos opuestos.
        let key = |v: [f32; 3]| v.map(|c| (c * 1000.0).round() as i64);
        let mut edges = std::collections::HashMap::new();
        for t in &out {
            for e in 0..3 {
                *edges.entry((key(t[e]), key(t[(e + 1) % 3]))).or_insert(0) += 1;
            }
        }
        for ((a, b), n) in &edges {
            assert_eq!(edges.get(&(*b, *a)), Some(n), "arista abierta");
        }
    }

    #[test]
    fn envolvente() {
        let h = convex_hull(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]]);
        assert_eq!(h.len(), 4);
    }
}
