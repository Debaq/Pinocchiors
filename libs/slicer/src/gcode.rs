//! Lectura del gcode de Orca para la vista previa: segmentos por capa y tipo.
//!
//! Orca marca el gcode con comentarios: `; CHANGE_LAYER` / `;LAYER_CHANGE`,
//! `; Z_HEIGHT: z` / `;Z:z`, `; LAYER_HEIGHT: h` / `;HEIGHT:h`,
//! `; FEATURE: Outer wall` / `;TYPE:Outer wall` y `; LINE_WIDTH: w` / `;WIDTH:w`.

use serde::Serialize;

/// Tipo de extrusión (mismo orden que el frontend: `lib/slicer.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum Feature {
    InnerWall = 0,
    OuterWall = 1,
    OverhangWall = 2,
    SparseInfill = 3,
    SolidInfill = 4,
    TopSurface = 5,
    BottomSurface = 6,
    Ironing = 7,
    Bridge = 8,
    InternalBridge = 9,
    GapInfill = 10,
    Skirt = 11,
    Brim = 12,
    Support = 13,
    SupportInterface = 14,
    SupportTransition = 15,
    PrimeTower = 16,
    Custom = 17,
    Other = 18,
}

impl Feature {
    pub const COUNT: usize = 19;

    fn from_name(name: &str) -> Self {
        match name.trim() {
            "Inner wall" | "Perimeter" => Self::InnerWall,
            "Outer wall" | "External perimeter" => Self::OuterWall,
            "Overhang wall" | "Overhang perimeter" => Self::OverhangWall,
            "Sparse infill" | "Internal infill" => Self::SparseInfill,
            "Internal solid infill" | "Solid infill" => Self::SolidInfill,
            "Top surface" | "Top solid infill" => Self::TopSurface,
            "Bottom surface" => Self::BottomSurface,
            "Ironing" => Self::Ironing,
            "Bridge" | "Bridge infill" => Self::Bridge,
            "Internal Bridge" | "Internal bridge" => Self::InternalBridge,
            "Gap infill" | "Gap fill" => Self::GapInfill,
            "Skirt" => Self::Skirt,
            "Brim" => Self::Brim,
            "Support" | "Support material" => Self::Support,
            "Support interface" | "Support material interface" => Self::SupportInterface,
            "Support transition" => Self::SupportTransition,
            "Prime tower" | "Wipe tower" => Self::PrimeTower,
            "Custom" => Self::Custom,
            _ => Self::Other,
        }
    }
}

/// Segmentos listos para dibujar. Los de extrusión y los de viaje van aparte;
/// `layer_start[i]` / `travel_start[i]` dicen dónde empieza la capa `i`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Preview {
    /// 6 floats por segmento: x0 y0 z0 x1 y1 z1 (mm, Z arriba)
    #[serde(skip)]
    pub positions: Vec<f32>,
    /// `Feature` de cada segmento
    #[serde(skip)]
    pub features: Vec<u8>,
    /// Ancho de línea de cada segmento (mm)
    #[serde(skip)]
    pub widths: Vec<f32>,
    /// 6 floats por viaje
    #[serde(skip)]
    pub travels: Vec<f32>,
    pub layer_z: Vec<f32>,
    pub layer_height: Vec<f32>,
    pub layer_start: Vec<u32>,
    pub travel_start: Vec<u32>,
    pub segments: u32,
    pub travel_segments: u32,
    /// Largo extruido por tipo (mm de trayectoria)
    pub length_by_feature: Vec<f64>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Preview {
    /// Empaqueta los arreglos en binario para mandarlos sin JSON:
    /// `[u32 segmentos][u32 viajes][f32 positions][f32 widths][f32 travels][u8 features]`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + 4 * (self.positions.len() + self.widths.len() + self.travels.len()) + self.features.len());
        out.extend_from_slice(&self.segments.to_le_bytes());
        out.extend_from_slice(&self.travel_segments.to_le_bytes());
        for v in self.positions.iter().chain(&self.widths).chain(&self.travels) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.features);
        out
    }
}

struct State {
    pos: [f64; 3],
    e: f64,
    relative_xyz: bool,
    relative_e: bool,
    feature: Feature,
    width: f32,
}

/// Lee el gcode completo. Los segmentos de menos de `min_len` mm seguidos del
/// mismo tipo se juntan (las curvas finas no necesitan miles de trocitos).
pub fn parse_gcode(text: &str) -> Preview {
    let mut p = Preview { length_by_feature: vec![0.0; Feature::COUNT], min: [f32::MAX; 3], max: [f32::MIN; 3], ..Default::default() };
    let mut st = State { pos: [0.0; 3], e: 0.0, relative_xyz: false, relative_e: false, feature: Feature::Other, width: 0.4 };
    let mut pending_height: Option<f32> = None;
    let mut layer_open = false;

    let new_layer = |p: &mut Preview, z: f32, h: f32| {
        p.layer_z.push(z);
        p.layer_height.push(h);
        p.layer_start.push(p.segments);
        p.travel_start.push(p.travel_segments);
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(c) = line.strip_prefix(';') {
            let c = c.trim_start();
            if c.starts_with("CHANGE_LAYER") || c.starts_with("LAYER_CHANGE") {
                layer_open = true;
                // Z llega en el comentario siguiente o en el primer movimiento.
                new_layer(&mut p, st.pos[2] as f32, pending_height.unwrap_or(0.0));
            } else if let Some(v) = c.strip_prefix("Z_HEIGHT:").or_else(|| c.strip_prefix("Z:")) {
                if let (Ok(z), Some(last)) = (v.trim().parse::<f32>(), p.layer_z.last_mut()) {
                    *last = z;
                }
            } else if let Some(v) = c.strip_prefix("LAYER_HEIGHT:").or_else(|| c.strip_prefix("HEIGHT:")) {
                if let Ok(h) = v.trim().parse::<f32>() {
                    pending_height = Some(h);
                    if let Some(last) = p.layer_height.last_mut() {
                        *last = h;
                    }
                }
            } else if let Some(v) = c.strip_prefix("FEATURE:").or_else(|| c.strip_prefix("TYPE:")) {
                st.feature = Feature::from_name(v);
            } else if let Some(v) = c.strip_prefix("LINE_WIDTH:").or_else(|| c.strip_prefix("WIDTH:"))
                && let Ok(w) = v.trim().parse::<f32>() {
                    st.width = w;
                }
            continue;
        }
        let code = line.split(';').next().unwrap_or("").trim();
        let mut words = code.split_ascii_whitespace();
        let Some(cmd) = words.next() else { continue };
        match cmd {
            "G90" => st.relative_xyz = false,
            "G91" => st.relative_xyz = true,
            "M82" => st.relative_e = false,
            "M83" => st.relative_e = true,
            "G92" => {
                for w in words {
                    if let Some(v) = w.strip_prefix('E').and_then(|v| v.parse::<f64>().ok()) {
                        st.e = v;
                    }
                }
            }
            "G0" | "G1" | "G2" | "G3" => {
                let mut target = st.pos;
                let mut e: Option<f64> = None;
                let (mut i, mut j) = (0.0, 0.0);
                for w in words {
                    let (k, v) = w.split_at(1);
                    let Ok(v) = v.parse::<f64>() else { continue };
                    match k {
                        "X" => target[0] = if st.relative_xyz { st.pos[0] + v } else { v },
                        "Y" => target[1] = if st.relative_xyz { st.pos[1] + v } else { v },
                        "Z" => target[2] = if st.relative_xyz { st.pos[2] + v } else { v },
                        "E" => e = Some(v),
                        "I" => i = v,
                        "J" => j = v,
                        _ => {}
                    }
                }
                let de = match e {
                    Some(v) if st.relative_e => v,
                    Some(v) => {
                        let d = v - st.e;
                        st.e = v;
                        d
                    }
                    None => 0.0,
                };
                if !layer_open && target[2] != st.pos[2] && de > 0.0 {
                    // gcode sin marcas de capa: se abre una al extruir a otra altura
                    new_layer(&mut p, target[2] as f32, pending_height.unwrap_or(0.0));
                    layer_open = true;
                }
                if let Some(last) = p.layer_z.last_mut()
                    && *last == 0.0
                    && target[2] > 0.0
                {
                    *last = target[2] as f32;
                }
                let moved_xy = target[0] != st.pos[0] || target[1] != st.pos[1];
                if moved_xy {
                    let points = if cmd == "G2" || cmd == "G3" { arc_points(st.pos, target, i, j, cmd == "G2") } else { vec![target] };
                    let mut from = st.pos;
                    for to in points {
                        if de > 0.0 && cmd != "G0" {
                            push_segment(&mut p, from, to, st.feature, st.width);
                        } else if !p.layer_start.is_empty() {
                            p.travels.extend([from[0] as f32, from[1] as f32, from[2] as f32, to[0] as f32, to[1] as f32, to[2] as f32]);
                            p.travel_segments += 1;
                        }
                        from = to;
                    }
                }
                st.pos = target;
            }
            _ => {}
        }
    }
    if p.segments == 0 {
        p.min = [0.0; 3];
        p.max = [0.0; 3];
    }
    p
}

fn push_segment(p: &mut Preview, a: [f64; 3], b: [f64; 3], feature: Feature, width: f32) {
    if p.layer_start.is_empty() {
        p.layer_z.push(a[2] as f32);
        p.layer_height.push(0.0);
        p.layer_start.push(0);
        p.travel_start.push(0);
    }
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
    p.length_by_feature[feature as usize] += len;
    // Juntar con el anterior si sigue la misma línea (mismo tipo, colineal y corto).
    let n = p.segments as usize;
    let layer_first = *p.layer_start.last().unwrap() as usize;
    if n > layer_first && p.features[n - 1] == feature as u8 && (p.widths[n - 1] - width).abs() < 1e-3 {
        let s = &p.positions[6 * (n - 1)..6 * n];
        let prev_end = [s[3] as f64, s[4] as f64, s[5] as f64];
        let prev_start = [s[0] as f64, s[1] as f64, s[2] as f64];
        if (prev_end[0] - a[0]).abs() < 1e-4 && (prev_end[1] - a[1]).abs() < 1e-4 && (prev_end[2] - a[2]).abs() < 1e-4 {
            let u = [prev_end[0] - prev_start[0], prev_end[1] - prev_start[1]];
            let v = [b[0] - a[0], b[1] - a[1]];
            let cross = (u[0] * v[1] - u[1] * v[0]).abs();
            let lu = (u[0] * u[0] + u[1] * u[1]).sqrt();
            let lv = (v[0] * v[0] + v[1] * v[1]).sqrt();
            if lu > 0.0 && lv > 0.0 && cross / (lu * lv) < 0.01 && u[0] * v[0] + u[1] * v[1] > 0.0 {
                let s = &mut p.positions[6 * (n - 1)..6 * n];
                s[3] = b[0] as f32;
                s[4] = b[1] as f32;
                s[5] = b[2] as f32;
                grow(p, b);
                return;
            }
        }
    }
    p.positions.extend([a[0] as f32, a[1] as f32, a[2] as f32, b[0] as f32, b[1] as f32, b[2] as f32]);
    p.features.push(feature as u8);
    p.widths.push(width);
    p.segments += 1;
    grow(p, a);
    grow(p, b);
}

fn grow(p: &mut Preview, q: [f64; 3]) {
    for (k, v) in q.iter().enumerate() {
        p.min[k] = p.min[k].min(*v as f32);
        p.max[k] = p.max[k].max(*v as f32);
    }
}

/// Puntos de un arco G2 (horario) / G3 (antihorario) con centro en `from + (i, j)`.
fn arc_points(from: [f64; 3], to: [f64; 3], i: f64, j: f64, clockwise: bool) -> Vec<[f64; 3]> {
    let c = [from[0] + i, from[1] + j];
    let r = (i * i + j * j).sqrt();
    if r < 1e-6 {
        return vec![to];
    }
    let a0 = (from[1] - c[1]).atan2(from[0] - c[0]);
    let a1 = (to[1] - c[1]).atan2(to[0] - c[0]);
    let tau = std::f64::consts::TAU;
    let mut sweep = a1 - a0;
    if clockwise {
        if sweep >= 0.0 {
            sweep -= tau;
        }
    } else if sweep <= 0.0 {
        sweep += tau;
    }
    // Un tramo cada ~0.5 mm de arco, mínimo 2.
    let steps = ((sweep.abs() * r / 0.5).ceil() as usize).clamp(2, 256);
    (1..=steps)
        .map(|k| {
            let t = k as f64 / steps as f64;
            if k == steps {
                return to;
            }
            let a = a0 + sweep * t;
            [c[0] + r * a.cos(), c[1] + r * a.sin(), from[2] + (to[2] - from[2]) * t]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
M83
G1 Z0.2 F600
; CHANGE_LAYER
; Z_HEIGHT: 0.2
; LAYER_HEIGHT: 0.2
; FEATURE: Outer wall
; LINE_WIDTH: 0.42
G1 X10 Y10 F3000
G1 X20 Y10 E0.5
G1 X30 Y10 E0.5
G1 X30 Y20 E0.5
G1 E-0.8
G0 X50 Y50
;TYPE:Sparse infill
;WIDTH:0.45
G1 X60 Y50 E0.4
; CHANGE_LAYER
; Z_HEIGHT: 0.4
; LAYER_HEIGHT: 0.2
G1 Z0.4
; FEATURE: Inner wall
G2 X70 Y50 I5 J0 E1.0
";

    #[test]
    fn capas_tipos_y_viajes() {
        let p = parse_gcode(SAMPLE);
        assert_eq!(p.layer_z, vec![0.2, 0.4]);
        assert_eq!(p.layer_height, vec![0.2, 0.2]);
        // X10→X30 se junta en uno (colineal), luego X30 Y10→Y20, luego el relleno
        assert_eq!(p.layer_start[1], 3);
        assert_eq!(p.features[0], Feature::OuterWall as u8);
        assert_eq!(p.features[2], Feature::SparseInfill as u8);
        assert!((p.widths[2] - 0.45).abs() < 1e-6);
        assert_eq!(&p.positions[0..6], &[10.0, 10.0, 0.2, 30.0, 10.0, 0.2]);
        // el arco se parte en varios tramos, todos de pared interna y a Z 0.4
        let arc = (p.segments - 3) as usize;
        assert!(arc >= 2);
        assert!(p.features[3..].iter().all(|f| *f == Feature::InnerWall as u8));
        assert!((p.positions[6 * 3 + 2] - 0.4).abs() < 1e-6);
        assert!(p.travel_segments >= 1);
        assert!((p.length_by_feature[Feature::OuterWall as usize] - 30.0).abs() < 1e-6);
    }

    #[test]
    fn extrusion_absoluta() {
        let g = "M82\nG92 E0\n;LAYER_CHANGE\n;Z:0.3\n;TYPE:Skirt\nG1 X0 Y0 Z0.3\nG1 X5 Y0 E1\nG1 X5 Y5 E1\nG1 X0 Y5 E2\n";
        let p = parse_gcode(g);
        // el segundo tramo no extruye (E igual): queda como viaje
        assert_eq!(p.segments, 2);
        assert_eq!(p.travel_segments, 1);
        assert_eq!(p.features[0], Feature::Skirt as u8);
    }

    #[test]
    fn empaquetado_binario() {
        let p = parse_gcode(SAMPLE);
        let b = p.to_bytes();
        let n = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
        let t = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
        assert_eq!(b.len(), 8 + 4 * (6 * n + n + 6 * t) + n);
    }
}
