//! Núcleo B-Rep de Pinocchiors sobre OpenCASCADE.
//!
//! [`Shape`] envuelve un `TopoDS_Shape` (copia barata: comparte la geometría).
//! Todas las operaciones devuelven una forma nueva; las de entrada no cambian.
//! Unidades: milímetros. Ángulos: radianes.
//!
//! Los índices de caras y aristas valen para *esa* forma: tras recalcular
//! cambian. Para referencias que sobrevivan (sketch sobre una cara, redondeo de
//! una arista) usar la geometría de [`FaceInfo`] / [`EdgeInfo`].
//!
//! Sin OpenCASCADE instalado el crate compila igual y todo devuelve
//! [`Error`] con "OpenCASCADE no disponible" ([`available`] dice cuál es el caso).

mod ffi;

use std::ffi::CStr;
use std::ptr::NonNull;

pub type P3 = [f64; 3];

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);

pub type Result<T> = std::result::Result<T, Error>;

fn last_error() -> Error {
    // SAFETY: el puente siempre devuelve un C string válido (thread-local).
    let msg = unsafe { CStr::from_ptr(ffi::cad_last_error()) }.to_string_lossy().into_owned();
    Error(if msg.is_empty() { "OpenCASCADE falló sin mensaje".into() } else { msg })
}

/// `true` si esta compilación incluye OpenCASCADE.
pub fn available() -> bool {
    unsafe { ffi::cad_available() != 0 }
}

/// Versión de OpenCASCADE enlazada ("" sin OCCT).
pub fn occt_version() -> String {
    unsafe { CStr::from_ptr(ffi::cad_occt_version()) }.to_string_lossy().into_owned()
}

/// Qué caras del resultado salieron de cada cara de las entradas de una
/// operación (ver [`with_history`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    /// Entrada i: caras de las entradas en orden (todas las de la primera
    /// forma, después las de la segunda…) y, en redondeo y chaflán, después
    /// una por arista elegida con las caras que generó.
    pub images: Vec<Vec<usize>>,
}

fn take_history() -> History {
    let mut h = std::mem::MaybeUninit::<ffi::CadHistory>::zeroed();
    unsafe { ffi::cad_take_history(h.as_mut_ptr()) };
    let mut h = unsafe { h.assume_init() };
    let n = h.n.max(0) as usize;
    let images = if n == 0 {
        Vec::new()
    } else {
        // SAFETY: el puente reservó n + 1 offsets y offsets[n] caras
        let off = unsafe { std::slice::from_raw_parts(h.offsets, n + 1) };
        let faces = unsafe { std::slice::from_raw_parts(h.faces, off[n] as usize) };
        off.windows(2).map(|w| faces[w[0] as usize..w[1] as usize].iter().map(|&f| f as usize).collect()).collect()
    };
    unsafe { ffi::cad_history_free(&mut h) };
    History { images }
}

/// Corre una operación y devuelve también su historia (qué caras del
/// resultado vienen de qué caras de las entradas). Solo para operaciones que la
/// registran: booleanas, unión múltiple, redondeo, chaflán, cáscara, desmolde,
/// transformar, espejar y cortar.
pub fn with_history(op: impl FnOnce() -> Result<Shape>) -> Result<(Shape, History)> {
    let _ = take_history();
    let shape = op()?;
    Ok((shape, take_history()))
}

/// Curva de un perfil. Todas en 3D: los sketches las ubican en su plano.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    Line(P3, P3),
    /// Arco que pasa por inicio, punto medio y fin.
    Arc(P3, P3, P3),
    Circle { center: P3, normal: P3, radius: f64 },
    /// Spline interpolada por los puntos (cerrada si el primero = el último).
    Spline(Vec<P3>),
    /// Spline abierta con la dirección de salida y de llegada impuestas.
    SplineEnds { points: Vec<P3>, start: P3, end: P3 },
    /// Elipse completa: `major` es la dirección del semieje `a`; `b` va a 90°.
    Ellipse { center: P3, normal: P3, major: P3, a: f64, b: f64 },
}

impl Curve {
    fn encode(&self, kinds: &mut Vec<i32>, counts: &mut Vec<i32>, data: &mut Vec<f64>) {
        let before = data.len();
        let kind = match self {
            Curve::Line(a, b) => {
                data.extend_from_slice(a);
                data.extend_from_slice(b);
                0
            }
            Curve::Arc(a, m, b) => {
                data.extend_from_slice(a);
                data.extend_from_slice(m);
                data.extend_from_slice(b);
                1
            }
            Curve::Circle { center, normal, radius } => {
                data.extend_from_slice(center);
                data.extend_from_slice(normal);
                data.push(*radius);
                2
            }
            Curve::Spline(pts) => {
                for p in pts {
                    data.extend_from_slice(p);
                }
                3
            }
            Curve::SplineEnds { points, start, end } => {
                for p in points {
                    data.extend_from_slice(p);
                }
                data.extend_from_slice(start);
                data.extend_from_slice(end);
                5
            }
            Curve::Ellipse { center, normal, major, a, b } => {
                data.extend_from_slice(center);
                data.extend_from_slice(normal);
                data.extend_from_slice(major);
                data.push(*a);
                data.push(*b);
                4
            }
        };
        kinds.push(kind);
        counts.push((data.len() - before) as i32);
    }
}

/// Sistema de coordenadas para primitivas: origen, eje Z y eje X.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub origin: P3,
    pub z: P3,
    pub x: P3,
}

impl Frame {
    pub const WORLD: Frame = Frame { origin: [0.0; 3], z: [0.0, 0.0, 1.0], x: [1.0, 0.0, 0.0] };

    pub fn at(origin: P3) -> Self {
        Frame { origin, ..Self::WORLD }
    }

    fn raw(&self) -> [f64; 9] {
        let [o, z, x] = [self.origin, self.z, self.x];
        [o[0], o[1], o[2], z[0], z[1], z[2], x[0], x[1], x[2]]
    }
}

/// Eje de revolución (o de rotación).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub origin: P3,
    pub dir: P3,
}

impl Axis {
    fn raw(&self) -> [f64; 6] {
        let [o, d] = [self.origin, self.dir];
        [o[0], o[1], o[2], d[0], d[1], d[2]]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Cut,
    Intersect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Null,
    Compound,
    CompSolid,
    Solid,
    Shell,
    Face,
    Wire,
    Edge,
    Vertex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    Torus,
    BSpline,
    Revolution,
    Extrusion,
    Offset,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    Line,
    Circle,
    Ellipse,
    BSpline,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceInfo {
    pub surface: SurfaceKind,
    pub area: f64,
    /// Centro de masa de la cara (puede caer fuera de ella).
    pub center: P3,
    /// Punto sobre la cara cercano al centro, y la normal saliente ahí.
    pub point: P3,
    pub normal: P3,
    /// Eje de cilindros/conos/toros; centro de esferas en `origin`.
    pub axis: Option<Axis>,
    pub radius: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeInfo {
    pub curve: CurveKind,
    pub length: f64,
    pub start: P3,
    pub end: P3,
    pub mid: P3,
    pub tangent: P3,
    /// Círculos: centro, eje y radio.
    pub circle: Option<(P3, P3, f64)>,
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassInfo {
    pub volume: f64,
    pub area: f64,
    pub center: P3,
    pub bbox_min: P3,
    pub bbox_max: P3,
    /// Momentos principales de inercia respecto al centro de masa con densidad
    /// 1 (mm⁵ con medidas en mm; × densidad = masa·largo²) y sus ejes.
    pub inertia: P3,
    pub axes: [P3; 3],
}

/// Qué es una línea de una vista proyectada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlrKind {
    Visible,
    /// Contorno (silueta) de una superficie curva, visible.
    VisibleOutline,
    Hidden,
    HiddenOutline,
    /// Arista entre caras tangentes (se suele dibujar fina o no dibujar).
    Smooth,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlrLine {
    pub kind: HlrKind,
    pub points: Vec<[f64; 2]>,
}

/// Malla de visualización de una forma.
#[derive(Debug, Clone, Default)]
pub struct Tessellation {
    pub positions: Vec<P3>,
    pub normals: Vec<P3>,
    pub triangles: Vec<[u32; 3]>,
    /// Cara de origen de cada triángulo (para elegir caras con el mouse).
    pub triangle_face: Vec<u32>,
    /// Polilínea de cada arista, en el orden de los índices de arista.
    pub edges: Vec<Vec<P3>>,
}

pub struct Shape(NonNull<ffi::CadShape>);

// SAFETY: TopoDS_Shape usa conteo de referencias atómico; una forma no se
// muta después de creada (salvo la triangulación cacheada al teselar, que
// OCCT protege). No es Sync: no teselar la misma forma desde dos hilos.
unsafe impl Send for Shape {}

impl Drop for Shape {
    fn drop(&mut self) {
        unsafe { ffi::cad_shape_free(self.0.as_ptr()) }
    }
}

impl Clone for Shape {
    fn clone(&self) -> Self {
        let p = unsafe { ffi::cad_shape_clone(self.ptr()) };
        Shape(NonNull::new(p).expect("clonar forma"))
    }
}

impl std::fmt::Debug for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shape({:?}, {} caras)", self.kind(), self.face_count())
    }
}

fn wrap(p: *mut ffi::CadShape) -> Result<Shape> {
    NonNull::new(p).map(Shape).ok_or_else(last_error)
}

fn idx(v: &[usize]) -> Vec<i32> {
    v.iter().map(|&i| i as i32).collect()
}

fn ptrs(shapes: &[Shape]) -> Vec<*const ffi::CadShape> {
    shapes.iter().map(|s| s.ptr()).collect()
}

impl Shape {
    fn ptr(&self) -> *const ffi::CadShape {
        self.0.as_ptr()
    }

    // --- Perfiles ---

    /// Cara plana: el primer lazo es el borde, los demás son agujeros.
    pub fn face(loops: &[Vec<Curve>]) -> Result<Shape> {
        let (mut kinds, mut counts, mut data) = (Vec::new(), Vec::new(), Vec::new());
        let mut sizes = Vec::new();
        for l in loops {
            sizes.push(l.len() as i32);
            for c in l {
                c.encode(&mut kinds, &mut counts, &mut data);
            }
        }
        wrap(unsafe {
            ffi::cad_make_face(kinds.as_ptr(), counts.as_ptr(), data.as_ptr(), sizes.as_ptr(), sizes.len() as i32)
        })
    }

    /// Alambre abierto o cerrado (trayectorias, perfiles de loft).
    pub fn wire(curves: &[Curve]) -> Result<Shape> {
        let (mut kinds, mut counts, mut data) = (Vec::new(), Vec::new(), Vec::new());
        for c in curves {
            c.encode(&mut kinds, &mut counts, &mut data);
        }
        wrap(unsafe { ffi::cad_make_wire(kinds.as_ptr(), counts.as_ptr(), data.as_ptr(), curves.len() as i32) })
    }

    /// Polígono cerrado como cara.
    pub fn polygon(points: &[P3]) -> Result<Shape> {
        let n = points.len();
        let curves: Vec<Curve> = (0..n).map(|i| Curve::Line(points[i], points[(i + 1) % n])).collect();
        Shape::face(&[curves])
    }

    // --- Primitivas ---

    pub fn make_box(frame: Frame, dx: f64, dy: f64, dz: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_make_box(frame.raw().as_ptr(), dx, dy, dz) })
    }

    pub fn cylinder(frame: Frame, radius: f64, height: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_make_cylinder(frame.raw().as_ptr(), radius, height) })
    }

    pub fn cone(frame: Frame, r1: f64, r2: f64, height: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_make_cone(frame.raw().as_ptr(), r1, r2, height) })
    }

    pub fn sphere(center: P3, radius: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_make_sphere(center.as_ptr(), radius) })
    }

    pub fn torus(frame: Frame, major: f64, minor: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_make_torus(frame.raw().as_ptr(), major, minor) })
    }

    // --- Operaciones ---

    /// Extruye un perfil (cara → sólido, alambre → superficie).
    pub fn prism(&self, v: P3) -> Result<Shape> {
        wrap(unsafe { ffi::cad_prism(self.ptr(), v[0], v[1], v[2]) })
    }

    /// Revoluciona un perfil; `angle` ≥ 2π da la vuelta completa.
    pub fn revolve(&self, axis: Axis, angle: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_revol(self.ptr(), axis.raw().as_ptr(), angle) })
    }

    /// Barre este perfil a lo largo de `spine` (alambre).
    pub fn sweep(&self, spine: &Shape) -> Result<Shape> {
        wrap(unsafe { ffi::cad_pipe(self.ptr(), spine.ptr()) })
    }

    /// Loft entre perfiles (alambres o caras).
    pub fn loft(sections: &[Shape], solid: bool, ruled: bool) -> Result<Shape> {
        let p = ptrs(sections);
        wrap(unsafe { ffi::cad_loft(p.as_ptr(), p.len() as i32, solid as i32, ruled as i32) })
    }

    pub fn boolean(&self, other: &Shape, op: BooleanOp) -> Result<Shape> {
        let op = match op {
            BooleanOp::Union => 0,
            BooleanOp::Cut => 1,
            BooleanOp::Intersect => 2,
        };
        wrap(unsafe { ffi::cad_boolean(self.ptr(), other.ptr(), op) })
    }

    pub fn union(&self, other: &Shape) -> Result<Shape> {
        self.boolean(other, BooleanOp::Union)
    }

    pub fn cut(&self, other: &Shape) -> Result<Shape> {
        self.boolean(other, BooleanOp::Cut)
    }

    pub fn intersect(&self, other: &Shape) -> Result<Shape> {
        self.boolean(other, BooleanOp::Intersect)
    }

    /// Une muchas formas en una sola operación (patrones).
    pub fn fuse_all(shapes: &[Shape]) -> Result<Shape> {
        let p = ptrs(shapes);
        wrap(unsafe { ffi::cad_fuse_many(p.as_ptr(), p.len() as i32) })
    }

    /// Agrupa formas sin fusionarlas (cuerpos separados).
    pub fn compound(shapes: &[Shape]) -> Result<Shape> {
        let p = ptrs(shapes);
        wrap(unsafe { ffi::cad_compound(p.as_ptr(), p.len() as i32) })
    }

    pub fn fillet(&self, edges: &[usize], radius: f64) -> Result<Shape> {
        self.fillet_variable(edges, radius, radius)
    }

    /// Redondeo de radio variable: `start` al comienzo de cada arista, `end` al final.
    pub fn fillet_variable(&self, edges: &[usize], start: f64, end: f64) -> Result<Shape> {
        let e = idx(edges);
        wrap(unsafe { ffi::cad_fillet(self.ptr(), e.as_ptr(), e.len() as i32, start, end) })
    }

    pub fn chamfer(&self, edges: &[usize], distance: f64) -> Result<Shape> {
        let e = idx(edges);
        wrap(unsafe { ffi::cad_chamfer(self.ptr(), e.as_ptr(), e.len() as i32, distance, distance, 0, 0) })
    }

    /// Chaflán asimétrico: `distance` sobre una cara (la otra con `flip`) y
    /// `distance2` sobre la otra.
    pub fn chamfer_two(&self, edges: &[usize], distance: f64, distance2: f64, flip: bool) -> Result<Shape> {
        let e = idx(edges);
        wrap(unsafe { ffi::cad_chamfer(self.ptr(), e.as_ptr(), e.len() as i32, distance, distance2, 1, flip as i32) })
    }

    /// Chaflán por distancia (sobre una cara; la otra con `flip`) y ángulo en grados.
    pub fn chamfer_angle(&self, edges: &[usize], distance: f64, degrees: f64, flip: bool) -> Result<Shape> {
        let e = idx(edges);
        wrap(unsafe { ffi::cad_chamfer(self.ptr(), e.as_ptr(), e.len() as i32, distance, degrees, 2, flip as i32) })
    }

    /// Ahueca quitando `open_faces`; grosor negativo crece hacia adentro.
    pub fn shell(&self, open_faces: &[usize], thickness: f64) -> Result<Shape> {
        let f = idx(open_faces);
        wrap(unsafe { ffi::cad_shell(self.ptr(), f.as_ptr(), f.len() as i32, thickness) })
    }

    /// Ángulo de desmolde para las caras indicadas.
    pub fn draft(&self, faces: &[usize], pull: P3, angle: f64, neutral_origin: P3, neutral_normal: P3) -> Result<Shape> {
        let f = idx(faces);
        wrap(unsafe {
            ffi::cad_draft(
                self.ptr(),
                f.as_ptr(),
                f.len() as i32,
                pull.as_ptr(),
                angle,
                neutral_origin.as_ptr(),
                neutral_normal.as_ptr(),
            )
        })
    }

    /// Transformación afín: matriz 3×4 por filas.
    pub fn transform(&self, m: [[f64; 4]; 3]) -> Result<Shape> {
        let flat: Vec<f64> = m.iter().flatten().copied().collect();
        wrap(unsafe { ffi::cad_transform(self.ptr(), flat.as_ptr()) })
    }

    pub fn translate(&self, v: P3) -> Result<Shape> {
        self.transform([[1.0, 0.0, 0.0, v[0]], [0.0, 1.0, 0.0, v[1]], [0.0, 0.0, 1.0, v[2]]])
    }

    /// Rotación de `angle` rad alrededor de `axis` (Rodrigues).
    pub fn rotate(&self, axis: Axis, angle: f64) -> Result<Shape> {
        self.transform(rotation_matrix(axis, angle))
    }

    pub fn mirror(&self, origin: P3, normal: P3) -> Result<Shape> {
        wrap(unsafe { ffi::cad_mirror(self.ptr(), origin.as_ptr(), normal.as_ptr()) })
    }

    /// Corta por un plano y conserva el lado hacia donde apunta `normal`.
    pub fn split_keep(&self, origin: P3, normal: P3) -> Result<Shape> {
        wrap(unsafe { ffi::cad_split_keep(self.ptr(), origin.as_ptr(), normal.as_ptr()) })
    }

    /// Sólido cosido a partir de triángulos. Lento con mallas grandes
    /// (una cara B-Rep por triángulo): pensado para piezas chicas o simplificadas.
    pub fn from_mesh(vertices: &[P3], triangles: &[[u32; 3]], tolerance: f64) -> Result<Shape> {
        let v: Vec<f64> = vertices.iter().flatten().copied().collect();
        let t: Vec<i32> = triangles.iter().flatten().map(|&i| i as i32).collect();
        wrap(unsafe {
            ffi::cad_from_mesh(v.as_ptr(), vertices.len() as i32, t.as_ptr(), triangles.len() as i32, tolerance)
        })
    }

    // --- Consultas ---

    pub fn kind(&self) -> ShapeKind {
        match unsafe { ffi::cad_shape_kind(self.ptr()) } {
            1 => ShapeKind::Compound,
            2 => ShapeKind::CompSolid,
            3 => ShapeKind::Solid,
            4 => ShapeKind::Shell,
            5 => ShapeKind::Face,
            6 => ShapeKind::Wire,
            7 => ShapeKind::Edge,
            8 => ShapeKind::Vertex,
            _ => ShapeKind::Null,
        }
    }

    /// Chequeo topológico y geométrico completo (`BRepCheck_Analyzer`).
    pub fn is_valid(&self) -> bool {
        unsafe { ffi::cad_shape_is_valid(self.ptr()) != 0 }
    }

    pub fn face_count(&self) -> usize {
        unsafe { ffi::cad_count_faces(self.ptr()) }.max(0) as usize
    }

    pub fn edge_count(&self) -> usize {
        unsafe { ffi::cad_count_edges(self.ptr()) }.max(0) as usize
    }

    pub fn face_info(&self, index: usize) -> Result<FaceInfo> {
        let mut r = ffi::CadFaceInfo::default();
        if unsafe { ffi::cad_face_info(self.ptr(), index as i32, &mut r) } == 0 {
            return Err(last_error());
        }
        let surface = match r.surface {
            0 => SurfaceKind::Plane,
            1 => SurfaceKind::Cylinder,
            2 => SurfaceKind::Cone,
            3 => SurfaceKind::Sphere,
            4 => SurfaceKind::Torus,
            5 => SurfaceKind::BSpline,
            6 => SurfaceKind::Revolution,
            7 => SurfaceKind::Extrusion,
            8 => SurfaceKind::Offset,
            _ => SurfaceKind::Other,
        };
        let analytic = matches!(
            surface,
            SurfaceKind::Cylinder | SurfaceKind::Cone | SurfaceKind::Sphere | SurfaceKind::Torus
        );
        Ok(FaceInfo {
            surface,
            area: r.area,
            center: r.center,
            point: r.point,
            normal: r.normal,
            axis: analytic.then_some(Axis { origin: r.axis_origin, dir: r.axis_dir }),
            radius: analytic.then_some(r.radius),
        })
    }

    pub fn faces(&self) -> Result<Vec<FaceInfo>> {
        (0..self.face_count()).map(|i| self.face_info(i)).collect()
    }

    pub fn edge_info(&self, index: usize) -> Result<EdgeInfo> {
        let mut r = ffi::CadEdgeInfo::default();
        if unsafe { ffi::cad_edge_info(self.ptr(), index as i32, &mut r) } == 0 {
            return Err(last_error());
        }
        let curve = match r.curve {
            0 => CurveKind::Line,
            1 => CurveKind::Circle,
            2 => CurveKind::Ellipse,
            3 => CurveKind::BSpline,
            _ => CurveKind::Other,
        };
        Ok(EdgeInfo {
            curve,
            length: r.length,
            start: r.start,
            end: r.end,
            mid: r.mid,
            tangent: r.tangent,
            circle: (curve == CurveKind::Circle).then_some((r.center, r.axis, r.radius)),
            closed: r.closed != 0,
        })
    }

    pub fn edges(&self) -> Result<Vec<EdgeInfo>> {
        (0..self.edge_count()).map(|i| self.edge_info(i)).collect()
    }

    /// Caras que comparten la arista (1 o 2).
    pub fn edge_faces(&self, edge: usize) -> Result<Vec<usize>> {
        let mut out = [-1i32; 2];
        let n = unsafe { ffi::cad_edge_faces(self.ptr(), edge as i32, out.as_mut_ptr()) };
        if n == 0 && edge >= self.edge_count() {
            return Err(last_error());
        }
        Ok(out[..n as usize].iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect())
    }

    /// Distancia exacta de `point` a la cara `index`.
    pub fn face_distance(&self, index: usize, point: P3) -> Option<f64> {
        let d = unsafe { ffi::cad_face_distance(self.ptr(), index as i32, point.as_ptr()) };
        (d >= 0.0).then_some(d)
    }

    /// Distancia exacta de `point` a la arista `index`.
    pub fn edge_distance(&self, index: usize, point: P3) -> Option<f64> {
        let d = unsafe { ffi::cad_edge_distance(self.ptr(), index as i32, point.as_ptr()) };
        (d >= 0.0).then_some(d)
    }

    /// La cara `index` como forma propia (para medir).
    pub fn face_shape(&self, index: usize) -> Result<Shape> {
        wrap(unsafe { ffi::cad_sub_shape(self.ptr(), 0, index as i32) })
    }

    /// La arista `index` como forma propia.
    pub fn edge_shape(&self, index: usize) -> Result<Shape> {
        wrap(unsafe { ffi::cad_sub_shape(self.ptr(), 1, index as i32) })
    }

    /// `n` puntos a distancias iguales a lo largo de este alambre, con su tangente.
    pub fn sample_curve(&self, n: usize) -> Result<Vec<(P3, P3)>> {
        let (mut p, mut t) = (vec![0.0; 3 * n], vec![0.0; 3 * n]);
        if unsafe { ffi::cad_wire_sample(self.ptr(), n as i32, p.as_mut_ptr(), t.as_mut_ptr()) } == 0 {
            return Err(last_error());
        }
        Ok((0..n).map(|i| ([p[3 * i], p[3 * i + 1], p[3 * i + 2]], [t[3 * i], t[3 * i + 1], t[3 * i + 2]])).collect())
    }

    /// Hélice (alambre) alrededor de `axis`.
    pub fn helix(axis: Axis, radius: f64, pitch: f64, turns: f64, left: bool) -> Result<Shape> {
        wrap(unsafe { ffi::cad_make_helix(axis.origin.as_ptr(), axis.dir.as_ptr(), radius, pitch, turns, left as i32) })
    }

    /// Macho roscado: núcleo de `r_minor` más el filete ISO de 60° hasta
    /// `r_major`, de `length` desde el origen del marco a lo largo de su Z
    /// (puntas planas). La cresta pasa por `origin + r·x`: la X del marco es
    /// la fase de la hélice.
    pub fn thread(frame: Frame, r_minor: f64, r_major: f64, pitch: f64, length: f64, left: bool) -> Result<Shape> {
        wrap(unsafe {
            ffi::cad_make_thread(frame.origin.as_ptr(), frame.z.as_ptr(), frame.x.as_ptr(), r_minor, r_major, pitch, length, left as i32, 0)
        })
    }

    /// Como [`Shape::thread`] pero con la punta del comienzo achaflanada a 45°
    /// desde el diámetro menor (la punta de un tornillo).
    pub fn thread_pointed(frame: Frame, r_minor: f64, r_major: f64, pitch: f64, length: f64, left: bool) -> Result<Shape> {
        wrap(unsafe {
            ffi::cad_make_thread(frame.origin.as_ptr(), frame.z.as_ptr(), frame.x.as_ptr(), r_minor, r_major, pitch, length, left as i32, 1)
        })
    }

    /// Sólido de espesor `thickness` a partir de esta cara (o caras), hacia su normal.
    pub fn thicken(&self, thickness: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_thicken(self.ptr(), thickness) })
    }

    /// Distancia hasta la primera cara que cruza la semirrecta desde `origin` hacia `dir`.
    pub fn ray_hit(&self, origin: P3, dir: P3) -> Option<f64> {
        let t = unsafe { ffi::cad_ray_hit(self.ptr(), origin.as_ptr(), dir.as_ptr()) };
        (t > 0.0).then_some(t)
    }

    /// Prisma de esta cara (plana) hacia su normal con las paredes inclinadas
    /// `angle` radianes (positivo: se angosta al subir).
    pub fn draft_prism(&self, height: f64, angle: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_draft_prism(self.ptr(), height, angle) })
    }

    /// Esta cara plana desplazada hacia afuera (o hacia adentro si es negativo).
    pub fn offset_face(&self, distance: f64) -> Result<Shape> {
        wrap(unsafe { ffi::cad_offset_face(self.ptr(), distance) })
    }

    /// Los sólidos de la forma, cada uno como forma propia.
    pub fn solids(&self) -> Result<Vec<Shape>> {
        let n = unsafe { ffi::cad_count_solids(self.ptr()) }.max(0) as usize;
        (0..n).map(|i| wrap(unsafe { ffi::cad_sub_shape(self.ptr(), 3, i as i32) })).collect()
    }

    /// Para cada cara de `part` (que salió de esta forma), su índice acá.
    pub fn face_indices_of(&self, part: &Shape) -> Result<Vec<Option<usize>>> {
        let mut out = vec![-1i32; part.face_count()];
        if !out.is_empty() && unsafe { ffi::cad_face_indices_in(self.ptr(), part.ptr(), out.as_mut_ptr()) } == 0 {
            return Err(last_error());
        }
        Ok(out.into_iter().map(|i| (i >= 0).then_some(i as usize)).collect())
    }

    pub fn vertex_count(&self) -> usize {
        unsafe { ffi::cad_count_vertices(self.ptr()) }.max(0) as usize
    }

    /// Coordenadas de los vértices (en el orden de sus índices).
    pub fn vertices(&self) -> Result<Vec<P3>> {
        (0..self.vertex_count())
            .map(|i| {
                let mut p = [0.0; 3];
                if unsafe { ffi::cad_vertex_point(self.ptr(), i as i32, p.as_mut_ptr()) } == 0 {
                    return Err(last_error());
                }
                Ok(p)
            })
            .collect()
    }

    /// Vértice suelto en un punto.
    pub fn vertex(p: P3) -> Result<Shape> {
        wrap(unsafe { ffi::cad_make_vertex(p.as_ptr()) })
    }

    /// Distancia mínima a otra forma y los puntos más cercanos (en `self` y en `other`).
    pub fn min_distance(&self, other: &Shape) -> Option<(f64, P3, P3)> {
        let (mut a, mut b) = ([0.0; 3], [0.0; 3]);
        let d = unsafe { ffi::cad_min_distance(self.ptr(), other.ptr(), a.as_mut_ptr(), b.as_mut_ptr()) };
        (d >= 0.0).then_some((d, a, b))
    }

    /// Las caras a cada lado de cada arista (`None` si la arista es borde libre).
    pub fn edge_face_pairs(&self) -> Result<Vec<[Option<usize>; 2]>> {
        let n = self.edge_count();
        let mut out = vec![-1i32; 2 * n];
        if n > 0 && unsafe { ffi::cad_edge_face_pairs(self.ptr(), out.as_mut_ptr()) } == 0 {
            return Err(last_error());
        }
        let f = |v: i32| (v >= 0).then_some(v as usize);
        Ok(out.as_chunks::<2>().0.iter().map(|c| [f(c[0]), f(c[1])]).collect())
    }

    /// Cara más cercana a `point` (índice, distancia). Con `normal`, solo caras
    /// cuya normal ahí forme un coseno ≥ `min_cos` con ella.
    pub fn closest_face(&self, point: P3, normal: Option<P3>, min_cos: f64) -> Option<(usize, f64)> {
        let mut d = 0.0;
        let n = normal.as_ref().map_or(std::ptr::null(), |n| n.as_ptr());
        let i = unsafe { ffi::cad_closest_face(self.ptr(), point.as_ptr(), n, min_cos, &mut d) };
        (i >= 0).then_some((i as usize, d))
    }

    /// Arista más cercana a `point`. Con `dir`, solo aristas paralelas
    /// (|cos| ≥ `min_cos`) en el punto más cercano.
    pub fn closest_edge(&self, point: P3, dir: Option<P3>, min_cos: f64) -> Option<(usize, f64)> {
        let mut d = 0.0;
        let t = dir.as_ref().map_or(std::ptr::null(), |t| t.as_ptr());
        let i = unsafe { ffi::cad_closest_edge(self.ptr(), point.as_ptr(), t, min_cos, &mut d) };
        (i >= 0).then_some((i as usize, d))
    }

    pub fn mass(&self) -> Result<MassInfo> {
        let mut r = ffi::CadMassInfo::default();
        if unsafe { ffi::cad_mass_info(self.ptr(), &mut r) } == 0 {
            return Err(last_error());
        }
        let a = r.axes;
        Ok(MassInfo {
            volume: r.volume,
            area: r.area,
            center: r.center,
            bbox_min: r.bbox_min,
            bbox_max: r.bbox_max,
            inertia: r.inertia,
            axes: [[a[0], a[1], a[2]], [a[3], a[4], a[5]], [a[6], a[7], a[8]]],
        })
    }

    /// Malla para mostrar. `linear` en mm (error máximo a la superficie),
    /// `angular` en radianes.
    pub fn tessellate(&self, linear: f64, angular: f64) -> Result<Tessellation> {
        let mut m = std::mem::MaybeUninit::<ffi::CadMesh>::zeroed();
        let ok = unsafe { ffi::cad_tessellate(self.ptr(), linear, angular, m.as_mut_ptr()) };
        let mut m = unsafe { m.assume_init() };
        if ok == 0 {
            return Err(last_error());
        }
        // SAFETY: el puente reservó exactamente estos tamaños.
        let t = unsafe {
            let pos = std::slice::from_raw_parts(m.positions, m.n_vertices * 3);
            let nor = std::slice::from_raw_parts(m.normals, m.n_vertices * 3);
            let tri = std::slice::from_raw_parts(m.triangles, m.n_triangles * 3);
            let tf = std::slice::from_raw_parts(m.triangle_face, m.n_triangles);
            let off = std::slice::from_raw_parts(m.edge_offsets, m.n_edges + 1);
            let ep = std::slice::from_raw_parts(m.edge_points, off[m.n_edges] * 3);
            let p3 = |s: &[f64]| s.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2]]).collect::<Vec<P3>>();
            Tessellation {
                positions: p3(pos),
                normals: p3(nor),
                triangles: tri.as_chunks::<3>().0.iter().map(|c| [c[0], c[1], c[2]]).collect(),
                triangle_face: tf.iter().map(|&f| f as u32).collect(),
                edges: off.windows(2).map(|w| p3(&ep[w[0] * 3..w[1] * 3])).collect(),
            }
        };
        unsafe { ffi::cad_mesh_free(&mut m) };
        Ok(t)
    }

    // --- Archivos ---

    pub fn to_step(&self) -> Result<Vec<u8>> {
        take_bytes(|out, len| unsafe { ffi::cad_write_step(self.ptr(), out, len) })
    }

    /// STEP con varias piezas, cada una con nombre y color opcional (r, g, b en 0..1).
    pub fn parts_to_step(parts: &[(&Shape, &str, Option<[f64; 3]>)]) -> Result<Vec<u8>> {
        let names: Vec<std::ffi::CString> =
            parts.iter().map(|(_, n, _)| std::ffi::CString::new(n.replace('\0', "")).unwrap_or_default()).collect();
        let name_ptrs: Vec<*const std::ffi::c_char> = names.iter().map(|n| n.as_ptr()).collect();
        let shapes: Vec<*const ffi::CadShape> = parts.iter().map(|(s, _, _)| s.ptr()).collect();
        let colors: Vec<f64> = parts.iter().flat_map(|(_, _, c)| c.unwrap_or([-1.0; 3])).collect();
        take_bytes(|out, len| unsafe {
            ffi::cad_write_step_parts(shapes.as_ptr(), name_ptrs.as_ptr(), colors.as_ptr(), parts.len() as i32, out, len)
        })
    }

    /// Líneas de la vista desde `eye` (hacia quien mira) con `xdir` a la derecha:
    /// aristas y contornos visibles y ocultos, como polilíneas en el plano de la vista.
    pub fn hlr(&self, eye: P3, xdir: P3, deflection: f64) -> Result<Vec<HlrLine>> {
        let bytes = take_bytes(|out, len| unsafe { ffi::cad_hlr(self.ptr(), eye.as_ptr(), xdir.as_ptr(), deflection, out, len) })?;
        let v: Vec<f64> = bytes.chunks_exact(8).map(|c| f64::from_ne_bytes(c.try_into().unwrap())).collect();
        let mut out = Vec::new();
        let mut k = 1;
        for _ in 0..v.first().copied().unwrap_or(0.0) as usize {
            let kind = match v[k] as u8 {
                0 => HlrKind::Visible,
                1 => HlrKind::VisibleOutline,
                2 => HlrKind::Hidden,
                3 => HlrKind::HiddenOutline,
                _ => HlrKind::Smooth,
            };
            let n = v[k + 1] as usize;
            let points = (0..n).map(|i| [v[k + 2 + 2 * i], v[k + 3 + 2 * i]]).collect();
            out.push(HlrLine { kind, points });
            k += 2 + 2 * n;
        }
        // Una oculta que cae entera sobre una visible no se dibuja (las aristas de
        // atrás de una caja vista de frente coinciden con las de adelante)
        let visible: Vec<[[f64; 2]; 2]> = out
            .iter()
            .filter(|l| matches!(l.kind, HlrKind::Visible | HlrKind::VisibleOutline | HlrKind::Smooth))
            .flat_map(|l| l.points.windows(2).map(|w| [w[0], w[1]]).collect::<Vec<_>>())
            .collect();
        let tol = deflection.max(1e-9) * 2.0;
        let on_visible = |p: &[f64; 2]| visible.iter().any(|seg| point_segment_distance(*p, seg[0], seg[1]) <= tol);
        out.retain(|l| {
            !matches!(l.kind, HlrKind::Hidden | HlrKind::HiddenOutline)
                || !(l.points.iter().all(on_visible) && l.points.windows(2).all(|w| on_visible(&[(w[0][0] + w[1][0]) / 2.0, (w[0][1] + w[1][1]) / 2.0])))
        });
        Ok(out)
    }

    pub fn from_step(data: &[u8]) -> Result<Shape> {
        wrap(unsafe { ffi::cad_read_step(data.as_ptr(), data.len()) })
    }

    /// Formato nativo de OCCT: exacto y rápido, para guardar formas importadas.
    pub fn to_brep(&self) -> Result<Vec<u8>> {
        take_bytes(|out, len| unsafe { ffi::cad_write_brep(self.ptr(), out, len) })
    }

    pub fn from_brep(data: &[u8]) -> Result<Shape> {
        wrap(unsafe { ffi::cad_read_brep(data.as_ptr(), data.len()) })
    }
}

fn take_bytes(f: impl FnOnce(*mut *mut u8, *mut usize) -> i32) -> Result<Vec<u8>> {
    let mut p: *mut u8 = std::ptr::null_mut();
    let mut len = 0usize;
    if f(&mut p, &mut len) == 0 || p.is_null() {
        return Err(last_error());
    }
    let v = unsafe { std::slice::from_raw_parts(p, len) }.to_vec();
    unsafe { ffi::cad_bytes_free(p) };
    Ok(v)
}

fn point_segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-24 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

/// Matriz 3×4 de rotación de `angle` alrededor de `axis`.
pub fn rotation_matrix(axis: Axis, angle: f64) -> [[f64; 4]; 3] {
    let d = axis.dir;
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let [x, y, z] = [d[0] / len, d[1] / len, d[2] / len];
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    let r = [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ];
    // Trasladar para rotar alrededor de axis.origin: p' = R(p − o) + o
    let o = axis.origin;
    let mut m = [[0.0; 4]; 3];
    for i in 0..3 {
        m[i][..3].copy_from_slice(&r[i]);
        m[i][3] = o[i] - (r[i][0] * o[0] + r[i][1] * o[1] + r[i][2] * o[2]);
    }
    m
}
