use crate::bvh::Ray;
use glam::Vec3;

/// Cámara de agujero (pinhole).
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    /// Mitad del alto del plano de imagen a distancia 1
    half_h: f32,
    half_w: f32,
}

impl Camera {
    /// `vfov_deg`: campo de visión vertical; `aspect` = ancho / alto
    pub fn look_at(position: Vec3, target: Vec3, up: Vec3, vfov_deg: f32, aspect: f32) -> Self {
        let forward = (target - position).normalize_or_zero();
        let right = forward.cross(up).normalize_or_zero();
        let up = right.cross(forward);
        let half_h = (vfov_deg.to_radians() * 0.5).tan();
        Camera { position, forward, right, up, half_h, half_w: half_h * aspect }
    }

    /// Rayo por el punto (x, y) de la imagen, en [0,1]² con origen arriba a la izquierda
    pub fn ray(&self, x: f32, y: f32) -> Ray {
        let sx = (2.0 * x - 1.0) * self.half_w;
        let sy = (1.0 - 2.0 * y) * self.half_h;
        Ray { origin: self.position, dir: (self.forward + self.right * sx + self.up * sy).normalize() }
    }
}
