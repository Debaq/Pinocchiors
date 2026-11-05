//! Texto del sketch en Rust (para el MCP): contornos de las letras de una
//! fuente TrueType a líneas y splines. Porta apps/web/src/lib/sketchText.ts
//! (`outlineContours`, `layoutText`) y `addText` de apps/web/src/lib/cad.ts,
//! de modo que la interfaz pueda rehacer el texto igual que uno propio.

use std::collections::HashMap;

use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::geom::P2;
use crate::sketch::{Geometry, Sketch, SketchText, TextStyle};

/// Alineación del texto respecto del ancla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error("la fuente no se pudo leer: {0}")]
    BadFont(String),
    #[error("el texto está vacío")]
    Empty,
    #[error("el tamaño del texto debe ser un número mayor que cero")]
    BadSize,
    #[error("la posición del texto no es un número válido")]
    BadPosition,
    #[error("el texto debe ir en una sola línea")]
    MultiLine,
    #[error("el texto no tiene letras con contorno")]
    NoOutline,
}

/// Qué se agregó al sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct TextAdded {
    /// Id del registro en `Sketch::texts`.
    pub id: u32,
    pub anchor: u32,
    pub entities: Vec<u32>,
}

/// Tramo de un contorno: recta, o spline que pasa por esos puntos.
enum Piece {
    Line(P2, P2),
    Spline(Vec<P2>),
}

enum Contour {
    Pieces(Vec<Piece>),
    Closed(Vec<P2>),
}

enum Seg {
    Line(P2, P2),
    /// Puntos por donde pasa, dirección de salida y de llegada.
    Curve(Vec<P2>, P2, P2),
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}
fn len(v: P2) -> f64 {
    v[0].hypot(v[1])
}

fn smooth_joint(a: &Seg, b: &Seg) -> bool {
    let (Seg::Curve(_, _, u), Seg::Curve(_, v, _)) = (a, b) else { return false };
    let d = len(*u) * len(*v);
    let cos = (u[0] * v[0] + u[1] * v[1]) / if d == 0.0 { 1.0 } else { d };
    cos > (15f64).to_radians().cos()
}

fn first_dir(dirs: &[P2]) -> P2 {
    dirs.iter().copied().find(|d| len(*d) > 1e-12).unwrap_or([1.0, 0.0])
}

fn contour_of(segs: Vec<Seg>) -> Option<Contour> {
    let n = segs.len();
    if n == 0 {
        return None;
    }
    let joint = |i: usize| smooth_joint(&segs[i], &segs[(i + 1) % n]);
    if segs.iter().all(|s| matches!(s, Seg::Curve(..))) && (0..n).all(joint) {
        let pts = segs
            .iter()
            .flat_map(|s| match s {
                Seg::Curve(p, ..) => p[..p.len() - 1].to_vec(),
                _ => vec![],
            })
            .collect();
        return Some(Contour::Closed(pts));
    }
    // Empezar después de un quiebre, así ninguna corrida cruza el inicio
    let brk = (0..n).find(|&i| !joint(i)).unwrap_or(0);
    let order: Vec<&Seg> = (0..n).map(|i| &segs[(brk + 1 + i) % n]).collect();
    let mut pieces = Vec::new();
    let mut run: Option<Vec<P2>> = None;
    for (i, s) in order.iter().enumerate() {
        match s {
            Seg::Line(a, b) => pieces.push(Piece::Line(*a, *b)),
            Seg::Curve(pts, ..) => {
                let r = match run.take() {
                    Some(mut r) => {
                        r.extend_from_slice(&pts[1..]);
                        r
                    }
                    None => pts.clone(),
                };
                match order.get(i + 1) {
                    Some(next) if smooth_joint(s, next) => run = Some(r),
                    _ => pieces.push(Piece::Spline(r)),
                }
            }
        }
    }
    Some(Contour::Pieces(pieces))
}

/// Recolecta el contorno de un glifo (y hacia arriba) con el origen en `at`.
struct Outliner {
    at: P2,
    scale: f64,
    out: Vec<Contour>,
    segs: Vec<Seg>,
    start: P2,
    cur: P2,
}

impl Outliner {
    fn map(&self, x: f32, y: f32) -> P2 {
        [self.at[0] + x as f64 * self.scale, self.at[1] + y as f64 * self.scale]
    }
    fn flush(&mut self) {
        if let Some(c) = contour_of(std::mem::take(&mut self.segs)) {
            self.out.push(c);
        }
    }
}

impl OutlineBuilder for Outliner {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        self.start = self.map(x, y);
        self.cur = self.start;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        if len(sub(p, self.cur)) > 1e-9 {
            self.segs.push(Seg::Line(self.cur, p));
        }
        self.cur = p;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, q, p) = (self.cur, self.map(x1, y1), self.map(x, y));
        let mid = [0.25 * c[0] + 0.5 * q[0] + 0.25 * p[0], 0.25 * c[1] + 0.5 * q[1] + 0.25 * p[1]];
        let t0 = first_dir(&[sub(q, c), sub(p, c)]);
        let t1 = first_dir(&[sub(p, q), sub(p, c)]);
        self.segs.push(Seg::Curve(vec![c, mid, p], t0, t1));
        self.cur = p;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c, c1, c2, p) = (self.cur, self.map(x1, y1), self.map(x2, y2), self.map(x, y));
        let at3 = |t: f64| {
            let u = 1.0 - t;
            let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            [k[0] * c[0] + k[1] * c1[0] + k[2] * c2[0] + k[3] * p[0], k[0] * c[1] + k[1] * c1[1] + k[2] * c2[1] + k[3] * p[1]]
        };
        let t0 = first_dir(&[sub(c1, c), sub(c2, c), sub(p, c)]);
        let t1 = first_dir(&[sub(p, c2), sub(p, c1), sub(p, c)]);
        self.segs.push(Seg::Curve(vec![c, at3(1.0 / 3.0), at3(2.0 / 3.0), p], t0, t1));
        self.cur = p;
    }
    fn close(&mut self) {
        if len(sub(self.start, self.cur)) > 1e-9 {
            self.segs.push(Seg::Line(self.cur, self.start));
        }
        self.cur = self.start;
        self.flush();
    }
}

/// Glifo de un carácter (el .notdef si la fuente no lo tiene, como opentype.js).
fn glyph(face: &Face, c: char) -> GlyphId {
    face.glyph_index(c).unwrap_or(GlyphId(0))
}

/// Ajuste de pareja (kern) en unidades de la fuente.
fn kerning(face: &Face, a: GlyphId, b: GlyphId) -> f64 {
    let Some(kern) = face.tables().kern else { return 0.0 };
    for st in kern.subtables {
        if st.horizontal && !st.variable && !st.has_cross_stream {
            if let Some(v) = st.glyphs_kerning(a, b) {
                return v as f64;
            }
        }
    }
    0.0
}

/// Avance total del texto (con kerning), en mm.
fn advance_width(face: &Face, text: &str, scale: f64) -> f64 {
    let mut x = 0.0;
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        let g = glyph(face, c);
        if let Some(p) = prev {
            x += kerning(face, p, g) * scale;
        }
        x += face.glyph_hor_advance(g).unwrap_or(0) as f64 * scale;
        prev = Some(g);
    }
    x
}

/// Contornos del texto con la línea base empezando en `at` (ya desplazada por la alineación).
fn layout(face: &Face, text: &str, size: f64, at: P2, align: TextAlign) -> Vec<Contour> {
    let scale = size / face.units_per_em() as f64;
    let width = advance_width(face, text, scale);
    let shift = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => -width / 2.0,
        TextAlign::Right => -width,
    };
    let mut x = at[0] + shift;
    let mut out = Vec::new();
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        let g = glyph(face, c);
        if let Some(p) = prev {
            x += kerning(face, p, g) * scale;
        }
        let mut o = Outliner { at: [x, at[1]], scale, out: Vec::new(), segs: Vec::new(), start: at, cur: at };
        if face.outline_glyph(g, &mut o).is_some() {
            o.flush();
            out.append(&mut o.out);
        }
        x += face.glyph_hor_advance(g).unwrap_or(0) as f64 * scale;
        prev = Some(g);
    }
    out
}

/// Agrega los contornos al sketch (como `addTextContours`): los tramos de un
/// mismo contorno comparten sus extremos. Devuelve las entidades creadas.
fn add_contours(s: &mut Sketch, contours: &[Contour]) -> Vec<u32> {
    let mut out = Vec::new();
    for c in contours {
        match c {
            Contour::Closed(pts) => {
                if pts.len() < 3 {
                    continue;
                }
                let ids = pts.iter().map(|p| s.add_point(p[0], p[1])).collect();
                out.push(s.add_entity(Geometry::Spline { points: ids, closed: true, start_handle: None, end_handle: None, handles: vec![] }));
            }
            Contour::Pieces(pieces) => {
                let mut ids: HashMap<String, u32> = HashMap::new();
                let mut at = |s: &mut Sketch, p: P2| -> u32 {
                    let k = format!("{:.9},{:.9}", p[0], p[1]);
                    *ids.entry(k).or_insert_with(|| s.add_point(p[0], p[1]))
                };
                for piece in pieces {
                    match piece {
                        Piece::Line(a, b) => {
                            let (start, end) = (at(s, *a), at(s, *b));
                            out.push(s.add_entity(Geometry::Line { start, end }));
                        }
                        Piece::Spline(pts) => {
                            let last = pts.len() - 1;
                            let ids2 = pts
                                .iter()
                                .enumerate()
                                .map(|(i, p)| if i == 0 || i == last { at(s, *p) } else { s.add_point(p[0], p[1]) })
                                .collect();
                            out.push(s.add_entity(Geometry::Spline { points: ids2, closed: false, start_handle: None, end_handle: None, handles: vec![] }));
                        }
                    }
                }
            }
        }
    }
    out
}

/// Agrega un texto al sketch igual que la interfaz: curvas (líneas y splines)
/// de los contornos de la fuente, el punto ancla en `at` (comienzo de la línea
/// base, alineación aparte) y el registro en `Sketch::texts` para rehacerlo.
///
/// `font_bytes` es un TTF/OTF con contornos; `font_name` es el nombre que se
/// guarda en el registro (la interfaz usa "Liberation Sans"; para negrita o
/// cursiva el llamador pasa el TTF correspondiente). `size` va en mm, igual que
/// en la interfaz (tamaño del em). Una sola línea; sin texto sobre curva.
/// No aplica ligaduras (solo kerning de la tabla `kern`).
pub fn add_text(
    sketch: &mut Sketch,
    font_bytes: &[u8],
    font_name: &str,
    text: &str,
    size: f64,
    at: P2,
    bold: bool,
    italic: bool,
    align: TextAlign,
) -> Result<TextAdded, TextError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(TextError::Empty);
    }
    if text.contains(['\n', '\r']) {
        return Err(TextError::MultiLine);
    }
    if !size.is_finite() || size <= 0.0 {
        return Err(TextError::BadSize);
    }
    if !at[0].is_finite() || !at[1].is_finite() {
        return Err(TextError::BadPosition);
    }
    let face = Face::parse(font_bytes, 0).map_err(|e| TextError::BadFont(e.to_string()))?;
    if face.units_per_em() == 0 {
        return Err(TextError::BadFont("unidades por em en cero".into()));
    }
    let contours = layout(&face, text, size, at, align);
    if contours.is_empty() {
        return Err(TextError::NoOutline);
    }
    let entities = add_contours(sketch, &contours);
    let mut used: Vec<u32> = Vec::new();
    for e in &entities {
        if let Some(ent) = sketch.entities.iter().find(|x| x.id == *e) {
            if let Geometry::Spline { points, .. } = &ent.geometry {
                used.extend(points);
            } else if let Geometry::Line { start, end } = &ent.geometry {
                used.extend([*start, *end]);
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    used.retain(|p| seen.insert(*p));
    // Como `sketchId`: un id nuevo (antes que el ancla, igual que en la interfaz)
    let id = sketch.add_point(0.0, 0.0);
    sketch.points.pop();
    let anchor = sketch.add_point(at[0], at[1]);
    let style = TextStyle { bold, italic, align: if align == TextAlign::Left { String::new() } else { align.name().into() }, path: None, frame: None };
    sketch.texts.push(SketchText { id, text: text.into(), size, font: font_name.into(), anchor, entities: entities.clone(), points: used, style, block: None });
    Ok(TextAdded { id, anchor, entities })
}
