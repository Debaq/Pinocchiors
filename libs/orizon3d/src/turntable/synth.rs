//! Banco sintético: renderiza lo que vería la webcam del rig (objeto sobre el
//! plato, luz del domo, líneas láser con su ancho y sombras, desenfoque, ruido
//! y el submuestreo de color del MJPG) e implementa [`Rig`] y
//! [`FrameSource`] sobre ese render. Como la forma es conocida, el error de
//! la reconstrucción se mide en mm contra ella.

use std::io;
use std::sync::{Arc, Mutex};

use nalgebra::{Isometry3, Point3};
use rayon::prelude::*;

use crate::camera::RgbFrame;
use crate::pointcloud::{Point, PointCloud};
use crate::register;

use super::Vec3;
use super::rig::RigGeometry;
use super::scan::{FrameSource, Rig};

/// Corte de un rayo con la superficie
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub t: f64,
    /// Normal unitaria hacia afuera
    pub normal: Vec3,
}

/// Objeto a escanear, en su propio marco (mm, Z arriba, apoyado en z = 0)
pub trait Surface: Sync + Send {
    /// Primer corte del rayo `origin + t·dir` (`dir` unitario) con `t` en
    /// `(0, t_max)`
    fn hit(&self, origin: &Vec3, dir: &Vec3, t_max: f64) -> Option<Hit>;
    /// Distancia de `p` a la superficie: la verdad contra la que se mide
    fn distance(&self, p: &Vec3) -> f64;
    fn albedo(&self, _p: &Vec3) -> [f64; 3] {
        [0.8, 0.8, 0.8]
    }
    /// Esfera que envuelve al objeto: centro y radio
    fn bounds(&self) -> (Vec3, f64);
}

/// Superficie dada por una función de distancia con signo, trazada por
/// sphere tracing dentro de una esfera envolvente
pub struct SdfSurface<F> {
    pub sdf: F,
    pub center: Vec3,
    pub radius: f64,
    pub color: [f64; 3],
}

impl<F: Fn(&Vec3) -> f64 + Sync + Send> Surface for SdfSurface<F> {
    fn hit(&self, origin: &Vec3, dir: &Vec3, t_max: f64) -> Option<Hit> {
        let oc = origin - self.center;
        let b = oc.dot(dir);
        let disc = b * b - (oc.norm_squared() - self.radius * self.radius);
        if disc < 0.0 {
            return None;
        }
        let root = disc.sqrt();
        let end = (-b + root).min(t_max);
        let mut t = (-b - root).max(0.0);
        for _ in 0..512 {
            if t > end {
                return None;
            }
            let d = (self.sdf)(&(origin + dir * t));
            if d < 1e-5 {
                let p = origin + dir * t;
                return Some(Hit { t, normal: self.normal(&p) });
            }
            t += d;
        }
        None
    }

    fn distance(&self, p: &Vec3) -> f64 {
        (self.sdf)(p).abs()
    }

    fn albedo(&self, _p: &Vec3) -> [f64; 3] {
        self.color
    }

    fn bounds(&self) -> (Vec3, f64) {
        (self.center, self.radius)
    }
}

impl<F: Fn(&Vec3) -> f64> SdfSurface<F> {
    fn normal(&self, p: &Vec3) -> Vec3 {
        let h = 1e-4;
        let d = |e: Vec3| (self.sdf)(&(p + e)) - (self.sdf)(&(p - e));
        Vec3::new(d(Vec3::x() * h), d(Vec3::y() * h), d(Vec3::z() * h)).normalize()
    }
}

fn sd_round_box(p: &Vec3, center: Vec3, half: Vec3, round: f64) -> f64 {
    let q = (p - center).abs() - half.add_scalar(-round);
    q.sup(&Vec3::zeros()).norm() + q.max().min(0.0) - round
}

fn sd_sphere(p: &Vec3, center: Vec3, r: f64) -> f64 {
    (p - center).norm() - r
}

/// Cilindro vertical de radio `r` entre `z0` y `z1`
fn sd_post(p: &Vec3, x: f64, y: f64, r: f64, z0: f64, z1: f64) -> f64 {
    let a = ((p.x - x).powi(2) + (p.y - y).powi(2)).sqrt() - r;
    let b = (p.z - (z0 + z1) / 2.0).abs() - (z1 - z0) / 2.0;
    a.max(0.0).hypot(b.max(0.0)) + a.max(b).min(0.0)
}

/// Objeto de prueba asimétrico de unos 40×30×42 mm apoyado en el plato: caja
/// redondeada con un mordisco esférico (concavidad), una esfera arriba y un
/// poste cilíndrico al costado
pub fn sample_object() -> SdfSurface<impl Fn(&Vec3) -> f64 + Sync + Send> {
    SdfSurface {
        sdf: |p: &Vec3| {
            let body = sd_round_box(p, Vec3::new(0.0, 0.0, 12.0), Vec3::new(18.0, 13.0, 12.0), 2.0);
            let body = body.max(-sd_sphere(p, Vec3::new(18.0, -4.0, 14.0), 7.0));
            let ball = sd_sphere(p, Vec3::new(6.0, -3.0, 30.0), 11.0);
            let post = sd_post(p, -12.0, 7.0, 5.0, 0.0, 42.0);
            body.min(ball).min(post)
        },
        center: Vec3::new(0.0, 0.0, 21.0),
        radius: 40.0,
        color: [0.8, 0.75, 0.7],
    }
}

/// Respuesta de la webcam
#[derive(Debug, Clone, Copy)]
pub struct Sensor {
    /// Escala de la radiancia: 1 = blanco con luz plena llega a 255
    pub exposure: f64,
    pub gamma: f64,
    /// Ruido gaussiano (niveles de 0 a 255)
    pub noise: f64,
    /// Desenfoque óptico (σ en px, 0 = nítido)
    pub blur: f64,
    /// Submuestreo de color 4:2:0 del MJPG (la única salida de la webcam)
    pub chroma_subsampling: bool,
}

impl Default for Sensor {
    fn default() -> Self {
        Sensor { exposure: 1.0, gamma: 2.2, noise: 2.0, blur: 0.7, chroma_subsampling: true }
    }
}

/// Láser de línea
#[derive(Debug, Clone, Copy)]
pub struct LaserLight {
    /// Irradiancia en el centro de la línea (1 = como la luz plena del domo)
    pub power: f64,
    /// σ del perfil gaussiano de la línea (mm)
    pub width: f64,
    pub color: [f64; 3],
}

impl Default for LaserLight {
    fn default() -> Self {
        // 650 nm: casi todo en el canal rojo del sensor
        LaserLight { power: 1.5, width: 0.25, color: [1.0, 0.1, 0.05] }
    }
}

/// Escena completa del banco
pub struct SynthScene<S> {
    pub object: S,
    pub rig: RigGeometry,
    pub plate_albedo: f64,
    /// Luz de la habitación que se cuela con el domo apagado (0 a 1)
    pub ambient: f64,
    pub sensor: Sensor,
    pub laser: LaserLight,
}

impl<S: Surface> SynthScene<S> {
    pub fn new(object: S, rig: RigGeometry) -> Self {
        SynthScene { object, rig, plate_albedo: 0.08, ambient: 0.03, sensor: Sensor::default(), laser: LaserLight::default() }
    }
}

/// Estado físico del rig en un cuadro
#[derive(Debug, Clone, PartialEq)]
pub struct RigState {
    pub plate: f64,
    pub elevation: f64,
    pub lasers: Vec<bool>,
    pub light: f64,
}

/// Datos de un cuadro que no cambian entre píxeles
struct Frame<'a, S> {
    scene: &'a SynthScene<S>,
    world_from_cam: Isometry3<f64>,
    object_from_world: Isometry3<f64>,
    lasers: Vec<(super::Plane, Vec3)>,
    light: f64,
}

/// Lado de la grilla con que se integra la línea láser dentro de un píxel
const SUPERSAMPLE: usize = 5;

impl<S: Surface> Frame<'_, S> {
    /// Primer corte del rayo del mundo con el objeto o el plato: punto,
    /// normal, albedo y `t`
    fn trace(&self, origin: &Vec3, dir: &Vec3) -> Option<(Vec3, Vec3, [f64; 3], f64)> {
        let rig = &self.scene.rig;
        let o = self.object_from_world * Point3::from(*origin);
        let d = self.object_from_world.rotation * dir;
        let mut best = self.scene.object.hit(&o.coords, &d, f64::INFINITY).map(|h| {
            let p = origin + dir * h.t;
            let n = self.object_from_world.inverse().rotation * h.normal;
            (p, n, self.scene.object.albedo(&(o.coords + d * h.t)), h.t)
        });
        let plate = super::Plane::from_point_normal(&rig.plate.point, &rig.plate.dir);
        if let Some(t) = plate.intersect(origin, dir)
            && t > 0.0
            && dir.dot(&rig.plate.dir) < 0.0
            && best.is_none_or(|b| t < b.3)
        {
            let p = origin + dir * t;
            if rig.plate.radius(&p) <= rig.plate_radius {
                best = Some((p, rig.plate.dir, [self.scene.plate_albedo; 3], t));
            }
        }
        best
    }

    /// ¿Ve el láser en `light` al punto `p` (en el mundo)?
    fn lit(&self, p: &Vec3, n: &Vec3, light: &Vec3) -> bool {
        let start = p + n * 0.01;
        let to = light - start;
        let dist = to.norm();
        let o = self.object_from_world * Point3::from(start);
        let d = self.object_from_world.rotation * (to / dist);
        self.scene.object.hit(&o.coords, &d, dist).is_none()
    }

    /// Radiancia lineal del píxel (u, v)
    fn shade(&self, u: f64, v: f64) -> [f64; 3] {
        let cam = &self.scene.rig.camera;
        let center = self.world_from_cam.translation.vector;
        let dir = self.world_from_cam.rotation * cam.ray(u, v);
        let Some((p, n, albedo, t)) = self.trace(&center, &dir) else {
            return [0.0; 3];
        };
        // Domo: luz difusa de todo el hemisferio, algo más fuerte de frente
        let facing = n.dot(&-dir).max(0.0);
        let diffuse = self.light * (0.3 + 0.7 * facing);
        let mut out = albedo.map(|a| a * diffuse);
        let laser = &self.scene.laser;
        for (plane, origin) in &self.lasers {
            let footprint = t / cam.fx / facing.max(0.1);
            if plane.distance(&p).abs() > 4.0 * laser.width + 2.0 * footprint {
                continue;
            }
            let l = (origin - p).normalize();
            let cos = n.dot(&l);
            if cos <= 0.0 || !self.lit(&p, &n, origin) {
                continue;
            }
            // La línea es más fina que el píxel en el borde: se integra su
            // perfil sobre el píxel cortando sub-rayos con el plano tangente
            let mut g = 0.0;
            for i in 0..SUPERSAMPLE {
                for j in 0..SUPERSAMPLE {
                    let du = (i as f64 + 0.5) / SUPERSAMPLE as f64 - 0.5;
                    let dv = (j as f64 + 0.5) / SUPERSAMPLE as f64 - 0.5;
                    let sub = self.world_from_cam.rotation * cam.ray(u + du, v + dv);
                    let den = n.dot(&sub);
                    if den.abs() < 1e-9 {
                        continue;
                    }
                    let q = center + sub * (n.dot(&(p - center)) / den);
                    let s = plane.distance(&q);
                    g += (-s * s / (2.0 * laser.width * laser.width)).exp();
                }
            }
            g /= (SUPERSAMPLE * SUPERSAMPLE) as f64;
            for c in 0..3 {
                out[c] += laser.color[c] * albedo[c] * laser.power * g * cos;
            }
        }
        out
    }
}

impl<S: Surface> SynthScene<S> {
    /// Cuadro que entrega la webcam con el rig en `state`. `seed` cambia el
    /// ruido entre cuadros
    pub fn render(&self, state: &RigState, seed: u64) -> RgbFrame {
        let rig = &self.rig;
        let frame = Frame {
            scene: self,
            world_from_cam: rig.world_from_camera(state.elevation),
            object_from_world: rig.plate.world_from_object(state.plate).inverse(),
            lasers: rig.lasers_at(state.elevation).into_iter().zip(&state.lasers).filter(|(_, on)| **on).map(|(l, _)| l).collect(),
            light: state.light + self.ambient,
        };
        let (w, h) = (rig.camera.width as usize, rig.camera.height as usize);
        let mut linear = vec![[0f64; 3]; w * h];
        linear.par_chunks_mut(w).enumerate().for_each(|(v, row)| {
            for (u, px) in row.iter_mut().enumerate() {
                *px = frame.shade(u as f64, v as f64);
            }
        });
        if self.sensor.blur > 0.0 {
            blur(&mut linear, w, h, self.sensor.blur);
        }
        let s = &self.sensor;
        let mut rgb: Vec<f64> = linear
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, px)| {
                (0..3).map(move |c| {
                    let x = (px[c] * s.exposure).max(0.0).powf(1.0 / s.gamma) * 255.0;
                    x + s.noise * gaussian(seed, (i * 3 + c) as u64)
                })
            })
            .collect();
        if s.chroma_subsampling {
            subsample_chroma(&mut rgb, w, h);
        }
        RgbFrame { width: w as u32, height: h as u32, rgb: rgb.iter().map(|x| x.round().clamp(0.0, 255.0) as u8).collect() }
    }
}

/// Desenfoque gaussiano separable sobre la radiancia lineal
fn blur(img: &mut [[f64; 3]], w: usize, h: usize, sigma: f64) {
    let r = (sigma * 3.0).ceil() as isize;
    let kernel: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f64 = kernel.iter().sum();
    let kernel: Vec<f64> = kernel.iter().map(|k| k / sum).collect();
    let pass = |src: &[[f64; 3]], len: usize, at: &(dyn Fn(usize, isize) -> Option<usize> + Sync)| -> Vec<[f64; 3]> {
        (0..len)
            .into_par_iter()
            .map(|i| {
                let mut acc = [0.0; 3];
                let mut wsum = 0.0;
                for (k, kw) in kernel.iter().enumerate() {
                    if let Some(j) = at(i, k as isize - r) {
                        for c in 0..3 {
                            acc[c] += src[j][c] * kw;
                        }
                        wsum += kw;
                    }
                }
                acc.map(|a| a / wsum)
            })
            .collect()
    };
    let horizontal = pass(img, w * h, &|i, d| {
        let x = (i % w) as isize + d;
        (0..w as isize).contains(&x).then(|| (i as isize + d) as usize)
    });
    let vertical = pass(&horizontal, w * h, &|i, d| {
        let y = (i / w) as isize + d;
        (0..h as isize).contains(&y).then(|| (i as isize + d * w as isize) as usize)
    });
    img.copy_from_slice(&vertical);
}

/// Promedia el color (Cb, Cr) en bloques de 2×2 y conserva la luminancia,
/// como el JPEG 4:2:0
fn subsample_chroma(rgb: &mut [f64], w: usize, h: usize) {
    let ycc = |p: &[f64]| {
        let y = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
        (y, -0.168736 * p[0] - 0.331264 * p[1] + 0.5 * p[2], 0.5 * p[0] - 0.418688 * p[1] - 0.081312 * p[2])
    };
    for by in (0..h).step_by(2) {
        for bx in (0..w).step_by(2) {
            let block: Vec<usize> =
                [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| (bx + dx, by + dy)).filter(|&(x, y)| x < w && y < h).map(|(x, y)| (y * w + x) * 3).collect();
            let (mut cb, mut cr) = (0.0, 0.0);
            for &i in &block {
                let (_, b, r) = ycc(&rgb[i..i + 3]);
                cb += b;
                cr += r;
            }
            let n = block.len() as f64;
            let (cb, cr) = (cb / n, cr / n);
            for &i in &block {
                let (y, _, _) = ycc(&rgb[i..i + 3]);
                rgb[i] = y + 1.402 * cr;
                rgb[i + 1] = y - 0.344136 * cb - 0.714136 * cr;
                rgb[i + 2] = y + 1.772 * cb;
            }
        }
    }
}

/// Número pseudoaleatorio reproducible en [0, 1) (splitmix64)
fn uniform(seed: u64, index: u64) -> f64 {
    let mut z = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ index.wrapping_add(0x6A09_E667_F3BC_C909);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Normal estándar reproducible (Box-Muller)
fn gaussian(seed: u64, index: u64) -> f64 {
    let a = uniform(seed, index * 2).max(1e-300);
    let b = uniform(seed, index * 2 + 1);
    (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
}

/// Errores de la mecánica que el banco puede simular
#[derive(Debug, Clone, Copy, Default)]
pub struct MotionErrors {
    /// Pasos del plato por vuelta (0 = continuo): el ángulo pedido se
    /// redondea al paso más cercano
    pub plate_steps: u32,
    /// σ del error de posición del plato en cada parada (grados)
    pub plate_jitter: f64,
    /// Error fijo del arco (grados)
    pub elevation_offset: f64,
}

struct SynthState {
    rig: RigState,
    frames: u64,
}

/// Rig sintético: mueve el estado de la escena; su [`SynthCamera`]
/// renderiza el cuadro
pub struct SynthRig<S> {
    scene: Arc<SynthScene<S>>,
    state: Arc<Mutex<SynthState>>,
    pub errors: MotionErrors,
    moves: u64,
}

pub struct SynthCamera<S> {
    scene: Arc<SynthScene<S>>,
    state: Arc<Mutex<SynthState>>,
}

impl<S: Surface> SynthRig<S> {
    pub fn new(scene: SynthScene<S>, errors: MotionErrors) -> Self {
        let rig = RigState { plate: 0.0, elevation: 0.0, lasers: vec![false; scene.rig.lasers.len()], light: 0.0 };
        SynthRig { scene: Arc::new(scene), state: Arc::new(Mutex::new(SynthState { rig, frames: 0 })), errors, moves: 0 }
    }

    pub fn camera(&self) -> SynthCamera<S> {
        SynthCamera { scene: self.scene.clone(), state: self.state.clone() }
    }

    pub fn scene(&self) -> &SynthScene<S> {
        &self.scene
    }

    fn with<T>(&self, f: impl FnOnce(&mut RigState) -> T) -> T {
        f(&mut self.state.lock().unwrap().rig)
    }
}

impl<S: Surface> Rig for SynthRig<S> {
    fn home(&mut self) -> io::Result<()> {
        self.move_to(0.0, 0.0)
    }

    fn move_to(&mut self, plate_deg: f64, elevation_deg: f64) -> io::Result<()> {
        let e = self.errors;
        let mut plate = plate_deg;
        if e.plate_steps > 0 {
            let step = 360.0 / e.plate_steps as f64;
            plate = (plate / step).round() * step;
        }
        self.moves += 1;
        plate += e.plate_jitter * gaussian(0xC0FFEE, self.moves);
        self.with(|s| {
            s.plate = plate;
            s.elevation = elevation_deg + e.elevation_offset;
        });
        Ok(())
    }

    fn set_laser(&mut self, index: usize, on: bool) -> io::Result<()> {
        self.with(|s| match s.lasers.get_mut(index) {
            Some(l) => {
                *l = on;
                Ok(())
            }
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("no hay láser {index}"))),
        })
    }

    fn set_light(&mut self, level: f64) -> io::Result<()> {
        self.with(|s| s.light = level.clamp(0.0, 1.0));
        Ok(())
    }
}

impl<S: Surface> FrameSource for SynthCamera<S> {
    fn grab(&mut self) -> io::Result<RgbFrame> {
        let (rig, seed) = {
            let mut st = self.state.lock().unwrap();
            st.frames += 1;
            (st.rig.clone(), st.frames)
        };
        Ok(self.scene.render(&rig, seed))
    }
}

/// Error de una nube contra la forma verdadera (mm)
#[derive(Debug, Clone, Copy, Default)]
pub struct Accuracy {
    pub points: usize,
    pub mean: f64,
    pub rms: f64,
    pub p95: f64,
    pub max: f64,
}

pub fn accuracy(cloud: &PointCloud, surface: &dyn Surface) -> Accuracy {
    let mut d: Vec<f64> =
        cloud.points.par_iter().map(|p| surface.distance(&Vec3::new(p.x as f64, p.y as f64, p.z as f64))).collect();
    if d.is_empty() {
        return Accuracy::default();
    }
    d.sort_by(f64::total_cmp);
    let n = d.len() as f64;
    Accuracy {
        points: d.len(),
        mean: d.iter().sum::<f64>() / n,
        rms: (d.iter().map(|x| x * x).sum::<f64>() / n).sqrt(),
        p95: d[((n * 0.95) as usize).min(d.len() - 1)],
        max: d[d.len() - 1],
    }
}

/// Puntos de la superficie que se pueden ver desde arriba del plato: los
/// cortes de rayos paralelos lanzados desde muchas direcciones
fn visible_samples(surface: &dyn Surface, min_height: f64) -> Vec<Vec3> {
    let (center, radius) = surface.bounds();
    const DIRECTIONS: usize = 400;
    const GRID: usize = 48;
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..DIRECTIONS)
        .into_par_iter()
        .flat_map_iter(|k| {
            let z = 1.0 - 2.0 * (k as f64 + 0.5) / DIRECTIONS as f64;
            let r = (1.0 - z * z).sqrt();
            let d = Vec3::new(r * (golden * k as f64).cos(), r * (golden * k as f64).sin(), z);
            let eye = center - d * 2.0 * radius;
            // Solo vistas desde arriba del plato
            let usable = eye.z > 0.0;
            let a = d.cross(&if d.z.abs() < 0.9 { Vec3::z() } else { Vec3::x() }).normalize();
            let b = d.cross(&a);
            (0..GRID * GRID).filter(move |_| usable).filter_map(move |i| {
                let (x, y) = ((i % GRID) as f64 + 0.5, (i / GRID) as f64 + 0.5);
                let o = eye + (a * (x / GRID as f64 - 0.5) + b * (y / GRID as f64 - 0.5)) * 2.0 * radius;
                let p = o + d * surface.hit(&o, &d, 4.0 * radius)?.t;
                (p.z >= min_height).then_some(p)
            })
        })
        .collect()
}

/// Fracción de la superficie visible desde arriba del plato (sin la base de
/// apoyo) que tiene un punto de la nube a menos de `reach` mm
pub fn coverage(cloud: &PointCloud, surface: &dyn Surface, reach: f64, min_height: f64) -> f64 {
    let samples = visible_samples(surface, min_height + reach);
    if samples.is_empty() {
        return 0.0;
    }
    let as_cloud = PointCloud {
        points: samples
            .iter()
            .map(|p| Point { x: p.x as f32, y: p.y as f32, z: p.z as f32, rgb: [0; 3], view: [0.0; 3] })
            .collect(),
        has_color: false,
    };
    let near = register::distances(&as_cloud, cloud, reach as f32);
    near.iter().filter(|d| d.is_some()).count() as f64 / near.len() as f64
}
