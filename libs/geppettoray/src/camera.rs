//! Cámara: agujero (pinhole) o lente fina real con distancia focal, apertura y
//! enfoque (profundidad de campo), forma de diafragma por aspas y desenfoque de
//! movimiento de la cámara.

use crate::bvh::Ray;
use crate::rng::Rng;
use glam::Vec3;
use std::f32::consts::PI;

/// Tamaño de sensor de referencia: fotograma completo 35 mm
pub const SENSOR_FULL_FRAME_HEIGHT_MM: f32 = 24.0;

/// Lente fina: abre el rayo sobre el disco de apertura y lo enfoca en el plano focal
#[derive(Debug, Clone, Copy)]
pub struct Lens {
    /// Radio de la apertura en unidades de la escena
    pub radius: f32,
    /// Distancia (a lo largo del eje de la cámara) del plano perfectamente nítido
    pub focus_distance: f32,
    /// Aspas del diafragma: 0 = apertura circular; ≥ 3 = polígono regular (bokeh poligonal)
    pub blades: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    /// Mitad del alto del plano de imagen a distancia 1
    half_h: f32,
    half_w: f32,
    lens: Option<Lens>,
    /// Posición y orientación al cierre del obturador, si la cámara se mueve
    motion_end: Option<([Vec3; 4], f32)>,
}

fn basis(position: Vec3, target: Vec3, up: Vec3) -> [Vec3; 4] {
    let forward = (target - position).normalize_or_zero();
    let right = forward.cross(up).normalize_or_zero();
    let up = right.cross(forward);
    [position, forward, right, up]
}

impl Camera {
    /// `vfov_deg`: campo de visión vertical; `aspect` = ancho / alto
    pub fn look_at(position: Vec3, target: Vec3, up: Vec3, vfov_deg: f32, aspect: f32) -> Self {
        let [position, forward, right, up] = basis(position, target, up);
        let half_h = (vfov_deg.to_radians() * 0.5).tan();
        Camera { position, forward, right, up, half_h, half_w: half_h * aspect, lens: None, motion_end: None }
    }

    /// Campo de visión vertical que da una distancia focal sobre un sensor de alto `sensor_height_mm`
    pub fn vfov_from_focal_length(focal_mm: f32, sensor_height_mm: f32) -> f32 {
        (2.0 * (sensor_height_mm / (2.0 * focal_mm)).atan()).to_degrees()
    }

    /// Cámara con distancia focal en mm (sensor de 24 mm de alto por defecto en
    /// [`SENSOR_FULL_FRAME_HEIGHT_MM`])
    pub fn from_focal_length(
        position: Vec3,
        target: Vec3,
        up: Vec3,
        focal_mm: f32,
        sensor_height_mm: f32,
        aspect: f32,
    ) -> Self {
        Self::look_at(position, target, up, Self::vfov_from_focal_length(focal_mm, sensor_height_mm), aspect)
    }

    /// Radio de apertura en unidades de escena: `focal / (2 N)`, con la focal en mm y
    /// `mm_to_units` los mm que vale una unidad de escena al revés (escena en metros → 0.001)
    pub fn aperture_radius(focal_mm: f32, f_number: f32, mm_to_units: f32) -> f32 {
        focal_mm / (2.0 * f_number.max(1e-3)) * mm_to_units
    }

    /// Activa la profundidad de campo
    pub fn with_lens(mut self, lens: Lens) -> Self {
        self.lens = Some(lens);
        self
    }

    /// Enfoca en un punto del espacio (el plano focal pasa por él)
    pub fn focused_on(self, point: Vec3, radius: f32, blades: u32) -> Self {
        let focus_distance = (point - self.position).dot(self.forward).max(1e-4);
        self.with_lens(Lens { radius, focus_distance, blades })
    }

    /// Desenfoque de movimiento: durante la exposición la cámara va de su pose actual a
    /// la que mira desde `end_position` hacia `end_target`
    pub fn with_motion(mut self, end_position: Vec3, end_target: Vec3, up: Vec3) -> Self {
        self.motion_end = Some((basis(end_position, end_target, up), 0.0));
        self
    }

    pub fn lens(&self) -> Option<&Lens> {
        self.lens.as_ref()
    }

    /// Rayo por el punto (x, y) de la imagen, en [0,1]² con origen arriba a la izquierda
    /// (pinhole, sin lente ni movimiento)
    pub fn ray(&self, x: f32, y: f32) -> Ray {
        let sx = (2.0 * x - 1.0) * self.half_w;
        let sy = (1.0 - 2.0 * y) * self.half_h;
        Ray { origin: self.position, dir: (self.forward + self.right * sx + self.up * sy).normalize() }
    }

    /// Rayo con lente y tiempo de obturador muestreados al azar
    pub fn sample_ray(&self, x: f32, y: f32, rng: &mut Rng) -> Ray {
        if self.lens.is_none() && self.motion_end.is_none() {
            return self.ray(x, y);
        }
        let (position, forward, right, up) = match &self.motion_end {
            Some((end, _)) => {
                let t = rng.next_f32();
                let pos = self.position.lerp(end[0], t);
                let f = self.forward.lerp(end[1], t).normalize_or_zero();
                let r = f.cross(self.up.lerp(end[3], t)).normalize_or_zero();
                (pos, f, r, r.cross(f))
            }
            None => (self.position, self.forward, self.right, self.up),
        };
        let sx = (2.0 * x - 1.0) * self.half_w;
        let sy = (1.0 - 2.0 * y) * self.half_h;
        let dir = forward + right * sx + up * sy; // z = 1 a lo largo de `forward`
        let Some(lens) = &self.lens else {
            return Ray { origin: position, dir: dir.normalize() };
        };
        // Punto nítido en el plano focal
        let focus = position + dir * lens.focus_distance;
        let (lx, ly) = sample_aperture(lens.blades, rng);
        let origin = position + right * (lx * lens.radius) + up * (ly * lens.radius);
        Ray { origin, dir: (focus - origin).normalize() }
    }
}

/// Punto en el disco de apertura unitario (o en el polígono regular inscrito en él)
fn sample_aperture(blades: u32, rng: &mut Rng) -> (f32, f32) {
    if blades < 3 {
        // Mapeo concéntrico de Shirley–Chiu
        let (a, b) = (2.0 * rng.next_f32() - 1.0, 2.0 * rng.next_f32() - 1.0);
        if a == 0.0 && b == 0.0 {
            return (0.0, 0.0);
        }
        let (r, phi) = if a.abs() > b.abs() { (a, PI / 4.0 * (b / a)) } else { (b, PI / 2.0 - PI / 4.0 * (a / b)) };
        return (r * phi.cos(), r * phi.sin());
    }
    // Triángulo del abanico (centro, vértice k, vértice k+1) uniforme
    let k = ((rng.next_f32() * blades as f32) as u32).min(blades - 1);
    let (a0, a1) = (2.0 * PI * k as f32 / blades as f32, 2.0 * PI * (k + 1) as f32 / blades as f32);
    let (mut u, mut v) = (rng.next_f32(), rng.next_f32());
    if u + v > 1.0 {
        u = 1.0 - u;
        v = 1.0 - v;
    }
    (u * a0.cos() + v * a1.cos(), u * a0.sin() + v * a1.sin())
}
